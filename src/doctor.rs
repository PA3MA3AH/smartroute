use crate::{
    config::{SmartRouteConfig, load_config, validate_config},
    singbox::generate_singbox_config,
};
use anyhow::Result;
use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    net::IpAddr,
    path::{Path, PathBuf},
    process::Command,
};

const ALLOWED_FINGERPRINTS: &[&str] = &[
    "chrome",
    "firefox",
    "edge",
    "safari",
    "360",
    "qq",
    "ios",
    "android",
    "random",
    "randomized",
];

const WHITELIST_GROUPS: &[(&str, &[&str])] = &[
    (
        "yandex",
        &[
            "yandex.ru",
            "yandex.net",
            "cdn.yandex.ru",
            "enterprise.api-maps.yandex.ru",
        ],
    ),
    ("ozon", &["ozon.ru", "ozone.ru", "ir.ozone.ru"]),
    ("wildberries", &["wildberries.ru", "wb.ru"]),
    (
        "gosuslugi",
        &["gosuslugi.ru", "esia.gosuslugi.ru", "gu-st.ru"],
    ),
    (
        "vk",
        &[
            "vk.com",
            "vk.ru",
            "userapi.com",
            "api.vk.ru",
            "pp.userapi.com",
        ],
    ),
    ("rutube", &["rutube.ru"]),
    (
        "mailru",
        &["mail.ru", "cloud.mail.ru", "cdn.mail.ru", "imgsmail.ru"],
    ),
    ("max", &["max.ru", "web.max.ru"]),
    (
        "sber",
        &[
            "sberbank.ru",
            "online.sberbank.ru",
            "cms-res-web.online.sberbank.ru",
        ],
    ),
];

#[derive(Default)]
struct DoctorReport {
    ok: usize,
    warn: usize,
    fail: usize,
}

impl DoctorReport {
    fn ok(&mut self, msg: impl AsRef<str>) {
        self.ok += 1;
        println!("[OK] {}", msg.as_ref());
    }

    fn warn(&mut self, msg: impl AsRef<str>) {
        self.warn += 1;
        println!("[WARN] {}", msg.as_ref());
    }

    fn fail(&mut self, msg: impl AsRef<str>) {
        self.fail += 1;
        println!("[FAIL] {}", msg.as_ref());
    }
}

pub fn doctor_config(input: &Path, strict: bool) -> Result<()> {
    println!("SmartRoute config doctor");
    println!("────────────────────────────────────────────────────────");
    println!("Config: {}", input.display());
    println!("Strict mode: {}", strict);
    println!();

    let mut report = DoctorReport::default();

    let config = match load_config(input) {
        Ok(config) => {
            report.ok("TOML parsed successfully");
            config
        }
        Err(err) => {
            report.fail(format!("failed to load config: {err:#}"));
            anyhow::bail!("doctor failed: config cannot be loaded");
        }
    };

    match validate_config(&config) {
        Ok(_) => report.ok("basic config validation passed"),
        Err(err) => report.fail(format!("basic config validation failed: {err:#}")),
    }

    check_general(&config, &mut report);
    check_subscription(&config, &mut report);
    check_nodes(&config, &mut report);
    check_chains(&config, &mut report);
    check_local_profiles(&config, &mut report);
    check_rules(&config, &mut report);
    check_generated_singbox(&config, &mut report);

    println!();
    println!("Summary:");
    println!("  OK: {}", report.ok);
    println!("  WARN: {}", report.warn);
    println!("  FAIL: {}", report.fail);

    if report.fail > 0 {
        anyhow::bail!("doctor failed: {} critical problem(s)", report.fail);
    }

    if strict && report.warn > 0 {
        anyhow::bail!("doctor strict failed: {} warning(s)", report.warn);
    }

    println!();
    report.ok("config looks good");

    Ok(())
}

fn check_general(config: &SmartRouteConfig, report: &mut DoctorReport) {
    println!();
    println!("General:");

    match config.general.mode.as_str() {
        "socks" | "tun" => report.ok(format!("mode = {}", config.general.mode)),
        other => report.fail(format!("unsupported mode: {}", other)),
    }

    if config.general.listen.trim().is_empty() {
        report.fail("general.listen is empty");
    } else {
        report.ok(format!(
            "default listen = {}:{}",
            config.general.listen, config.general.listen_port
        ));
    }

    if config.general.final_outbound == "direct" {
        report.fail("final_outbound = direct is forbidden in proxy-only mode");
    } else if known_outbounds(config).contains(&config.general.final_outbound) {
        report.ok(format!(
            "final_outbound exists: {}",
            config.general.final_outbound
        ));
    } else {
        report.fail(format!(
            "final_outbound points to unknown outbound: {}",
            config.general.final_outbound
        ));
    }
}

fn check_subscription(config: &SmartRouteConfig, report: &mut DoctorReport) {
    println!();
    println!("Subscription:");

    if config.subscription.auto_refresh == 0 {
        report.ok("subscription auto-refresh disabled");
        return;
    }

    match config.subscription.url.as_deref() {
        Some(url) if !url.trim().is_empty() => {
            report.ok(format!(
                "subscription auto-refresh every {}s",
                config.subscription.auto_refresh
            ));
            report.ok(format!("subscription URL configured: {}", redact_url(url)));
        }
        _ => {
            report.warn(format!(
                "auto_refresh = {}, but subscription.url is empty",
                config.subscription.auto_refresh
            ));
        }
    }
}

fn check_nodes(config: &SmartRouteConfig, report: &mut DoctorReport) {
    println!();
    println!("Nodes:");

    if config.nodes.is_empty() {
        report.fail("no proxy nodes configured");
        return;
    }

    let mut tags = HashSet::new();
    let mut whitelist_known = 0usize;
    let mut whitelist_unknown = 0usize;
    let mut domain_servers = 0usize;

    for node in &config.nodes {
        if node.tag.trim().is_empty() {
            report.fail("node with empty tag");
            continue;
        }

        if !tags.insert(node.tag.clone()) {
            report.fail(format!("duplicate node tag: {}", node.tag));
        }

        match node.node_type.as_str() {
            "socks" | "vless" => {}
            other => report.fail(format!("node {} has unsupported type: {}", node.tag, other)),
        }

        if node.server.trim().is_empty() {
            report.fail(format!("node {} has empty server", node.tag));
        } else if node.server.parse::<IpAddr>().is_err() {
            domain_servers += 1;
            report.warn(format!(
                "node {} uses domain server = {}; run resolve-domains",
                node.tag, node.server
            ));
        }

        if node.port == 0 {
            report.fail(format!("node {} has invalid port 0", node.tag));
        }

        if node.node_type == "vless" {
            if node.uuid.as_deref().unwrap_or("").trim().is_empty() {
                report.fail(format!("VLESS node {} has empty uuid", node.tag));
            }

            match node.security.as_deref() {
                Some("reality") => {
                    if node.server_name.as_deref().unwrap_or("").trim().is_empty() {
                        report.fail(format!("Reality node {} has empty server_name", node.tag));
                    }

                    if node
                        .reality_public_key
                        .as_deref()
                        .unwrap_or("")
                        .trim()
                        .is_empty()
                    {
                        report.fail(format!(
                            "Reality node {} has empty reality_public_key",
                            node.tag
                        ));
                    }

                    if let Some(server_name) = node.server_name.as_deref() {
                        match classify_sni(server_name) {
                            Some(_) => whitelist_known += 1,
                            None => whitelist_unknown += 1,
                        }
                    }
                }
                Some("tls") | None => {}
                Some(other) => report.warn(format!(
                    "VLESS node {} uses uncommon security = {}",
                    node.tag, other
                )),
            }

            if let Some(fp) = node.utls_fingerprint.as_deref() {
                if !ALLOWED_FINGERPRINTS.contains(&fp) {
                    report.warn(format!(
                        "node {} uses unknown utls_fingerprint = {}",
                        node.tag, fp
                    ));
                }
            }

            let tag_lower = node.tag.to_ascii_lowercase();
            if tag_lower.contains("grpc") {
                report.warn(format!(
                    "node {} looks like gRPC by tag, but transport support must be configured explicitly",
                    node.tag
                ));
            }

            if tag_lower.contains("ws") || tag_lower.contains("websocket") {
                report.warn(format!(
                    "node {} looks like WebSocket by tag, but transport support must be configured explicitly",
                    node.tag
                ));
            }
        }
    }

    if domain_servers == 0 {
        report.ok("all node servers are IP addresses");
    }

    report.ok(format!("nodes configured: {}", config.nodes.len()));
    report.ok(format!(
        "whitelist-compatible Reality masks: {}",
        whitelist_known
    ));

    if whitelist_unknown > 0 {
        report.warn(format!(
            "Reality masks outside known whitelist groups: {}",
            whitelist_unknown
        ));
    }
}

fn check_chains(config: &SmartRouteConfig, report: &mut DoctorReport) {
    println!();
    println!("Chains:");

    if config.chains.is_empty() {
        report.warn("no chains configured");
        return;
    }

    let base = base_outbounds(config);
    let all = known_outbounds(config);
    let mut tags = HashSet::new();

    for chain in &config.chains {
        if chain.tag.trim().is_empty() {
            report.fail("chain with empty tag");
            continue;
        }

        if !tags.insert(chain.tag.clone()) {
            report.fail(format!("duplicate chain tag: {}", chain.tag));
        }

        if chain.outbounds.len() < 2 {
            report.fail(format!("chain {} has less than 2 hops", chain.tag));
        }

        if chain.outbounds.iter().any(|hop| hop == "direct") {
            report.fail(format!(
                "chain {} references direct; proxy-only mode forbids this",
                chain.tag
            ));
        }

        for hop in &chain.outbounds {
            if hop == &chain.tag {
                report.fail(format!("chain {} references itself", chain.tag));
            }

            if !base.contains(hop) {
                report.fail(format!(
                    "chain {} references unknown base outbound: {}",
                    chain.tag, hop
                ));
            }
        }

        if all.contains(&chain.tag) {
            report.ok(format!(
                "chain {} = {}",
                chain.tag,
                chain.outbounds.join(" -> ")
            ));
        }
    }
}

fn check_local_profiles(config: &SmartRouteConfig, report: &mut DoctorReport) {
    println!();
    println!("Local profiles:");

    let all = known_outbounds(config);
    let mut tags = HashSet::new();
    let mut ports = HashSet::new();

    ports.insert(config.general.listen_port);

    if config.local_profiles.is_empty() {
        report.ok("no extra local profiles");
        return;
    }

    for profile in &config.local_profiles {
        if !tags.insert(profile.tag.clone()) {
            report.fail(format!("duplicate local profile tag: {}", profile.tag));
        }

        if !ports.insert(profile.listen_port) {
            report.fail(format!(
                "duplicate local listen port: {}",
                profile.listen_port
            ));
        }

        if !all.contains(&profile.outbound) {
            report.fail(format!(
                "local profile {} points to unknown outbound: {}",
                profile.tag, profile.outbound
            ));
        }

        if profile.outbound == "direct" {
            report.fail(format!(
                "local profile {} uses direct outbound",
                profile.tag
            ));
        }
    }

    report.ok(format!(
        "local profiles configured: {}",
        config.local_profiles.len()
    ));
}

fn check_rules(config: &SmartRouteConfig, report: &mut DoctorReport) {
    println!();
    println!("Rules:");

    let all = known_outbounds(config);
    let mut seen_exact = HashMap::<(String, String), String>::new();
    let mut duplicate_count = 0usize;
    let mut direct_count = 0usize;

    for rule in &config.rules {
        match rule.rule_type.as_str() {
            "domain" | "domain_suffix" | "domain_keyword" => {}
            other => report.fail(format!(
                "rule {} {} has unsupported type",
                other, rule.value
            )),
        }

        if rule.value.trim().is_empty() {
            report.fail("rule with empty value");
        }

        if !all.contains(&rule.outbound) {
            report.fail(format!(
                "rule {} {} points to unknown outbound: {}",
                rule.rule_type, rule.value, rule.outbound
            ));
        }

        if rule.outbound == "direct" {
            direct_count += 1;
            report.fail(format!(
                "rule {} {} uses direct outbound",
                rule.rule_type, rule.value
            ));
        }

        let key = (rule.rule_type.clone(), rule.value.clone());

        if let Some(prev) = seen_exact.insert(key, rule.outbound.clone()) {
            duplicate_count += 1;

            if prev == rule.outbound {
                report.warn(format!(
                    "duplicate rule {} {} -> {}",
                    rule.rule_type, rule.value, rule.outbound
                ));
            } else {
                report.fail(format!(
                    "conflicting duplicate rule {} {}: {} vs {}",
                    rule.rule_type, rule.value, prev, rule.outbound
                ));
            }
        }
    }

    if direct_count == 0 {
        report.ok("no rules use outbound = direct");
    }

    if duplicate_count == 0 {
        report.ok("no duplicate exact rules");
    }

    report.ok(format!("rules configured: {}", config.rules.len()));

    warn_shadowed_suffix_rules(config, report);
}

fn warn_shadowed_suffix_rules(config: &SmartRouteConfig, report: &mut DoctorReport) {
    let suffix_rules = config
        .rules
        .iter()
        .filter(|rule| rule.rule_type == "domain_suffix")
        .collect::<Vec<_>>();

    for (idx, a) in suffix_rules.iter().enumerate() {
        for b in suffix_rules.iter().skip(idx + 1) {
            if a.value == b.value {
                continue;
            }

            if is_same_or_subdomain(&a.value, &b.value) && a.outbound != b.outbound {
                report.warn(format!(
                    "domain_suffix {} -> {} may be shadowed by broader {} -> {} depending on route order",
                    a.value, a.outbound, b.value, b.outbound
                ));
            }

            if is_same_or_subdomain(&b.value, &a.value) && a.outbound != b.outbound {
                report.warn(format!(
                    "domain_suffix {} -> {} may be shadowed by broader {} -> {} depending on route order",
                    b.value, b.outbound, a.value, a.outbound
                ));
            }
        }
    }
}

fn check_generated_singbox(config: &SmartRouteConfig, report: &mut DoctorReport) {
    println!();
    println!("Generated sing-box:");

    match generate_singbox_config(config) {
        Ok(value) => {
            report.ok("SmartRoute generated sing-box JSON");

            let raw = match serde_json::to_string_pretty(&value) {
                Ok(raw) => raw,
                Err(err) => {
                    report.fail(format!("failed to serialize generated JSON: {err:#}"));
                    return;
                }
            };

            let path = temp_singbox_config_path();

            if let Err(err) = fs::write(&path, raw) {
                report.warn(format!("failed to write temp sing-box config: {err:#}"));
                return;
            }

            match Command::new("sing-box")
                .arg("check")
                .arg("-c")
                .arg(&path)
                .output()
            {
                Ok(output) if output.status.success() => {
                    report.ok("sing-box check passed");
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    report.fail(format!("sing-box check failed: {}", stderr.trim()));
                }
                Err(err) => {
                    report.warn(format!("failed to run sing-box check: {err:#}"));
                }
            }

            let _ = fs::remove_file(&path);
        }
        Err(err) => {
            report.fail(format!("failed to generate sing-box config: {err:#}"));
        }
    }
}

fn base_outbounds(config: &SmartRouteConfig) -> BTreeSet<String> {
    let mut outbounds = BTreeSet::new();

    outbounds.insert("direct".to_string());
    outbounds.insert("block".to_string());

    for node in &config.nodes {
        outbounds.insert(node.tag.clone());
    }

    outbounds
}

fn known_outbounds(config: &SmartRouteConfig) -> BTreeSet<String> {
    let mut outbounds = base_outbounds(config);

    for chain in &config.chains {
        outbounds.insert(chain.tag.clone());
    }

    outbounds
}

fn classify_sni(sni: &str) -> Option<&'static str> {
    let sni = sni.trim().trim_end_matches('.').to_ascii_lowercase();

    for (group, domains) in WHITELIST_GROUPS {
        for domain in *domains {
            if domain_match(&sni, domain) {
                return Some(*group);
            }
        }
    }

    None
}

fn domain_match(name: &str, base: &str) -> bool {
    let base = base.trim().trim_end_matches('.').to_ascii_lowercase();
    name == base || name.ends_with(&format!(".{}", base))
}

fn is_same_or_subdomain(name: &str, base: &str) -> bool {
    let name = name.trim().trim_end_matches('.').to_ascii_lowercase();
    let base = base.trim().trim_end_matches('.').to_ascii_lowercase();

    name == base || name.ends_with(&format!(".{}", base))
}

fn temp_singbox_config_path() -> PathBuf {
    PathBuf::from(format!(
        "/tmp/smartroute-doctor-singbox-{}.json",
        std::process::id()
    ))
}

fn redact_url(url: &str) -> String {
    if url.len() <= 48 {
        return url.to_string();
    }

    format!("{}...{}", &url[..24], &url[url.len() - 12..])
}
