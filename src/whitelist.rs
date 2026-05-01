use crate::config::{SmartRouteConfig, load_config};
use anyhow::{Context, Result};
use std::{
    collections::{BTreeSet, HashSet},
    net::IpAddr,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

const ALLOWED_GROUPS: &[(&str, &[&str])] = &[
    (
        "yandex",
        &[
            "yandex.ru",
            "yandex.net",
            "cdn.yandex.ru",
            "maps.yandex.ru",
            "enterprise.api-maps.yandex.ru",
        ],
    ),
    ("ozon", &["ozon.ru", "ozone.ru", "ir.ozone.ru"]),
    (
        "wildberries",
        &["wildberries.ru", "wb.ru", "static-basket-01.wb.ru"],
    ),
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

pub fn list_whitelist_masks(input: &Path) -> Result<()> {
    let config = load_config(input)?;

    println!("Whitelist-compatible Reality masks:");
    println!("────────────────────────────────────────────────────────");

    let mut compatible = 0usize;
    let mut unknown = 0usize;

    for node in &config.nodes {
        if node.node_type != "vless" {
            continue;
        }

        let Some(server_name) = node.server_name.as_deref() else {
            continue;
        };

        let group = classify_sni(server_name);

        match group {
            Some(group) => {
                compatible += 1;
                println!("[OK] {} -> {}", node.tag, group);
            }
            None => {
                unknown += 1;
                println!("[??] {} -> unknown", node.tag);
            }
        }

        println!("     server: {}:{}", node.server, node.port);
        println!("     server_name: {}", server_name);
        println!(
            "     fingerprint: {}",
            node.utls_fingerprint.as_deref().unwrap_or("chrome")
        );
        println!();
    }

    println!("Summary:");
    println!("  compatible masks: {}", compatible);
    println!("  unknown masks: {}", unknown);

    Ok(())
}

pub fn run_whitelist_test(input: &Path, domain: &str, interface: Option<&str>) -> Result<()> {
    let config = load_config(input)?;
    let host = normalize_host(domain);
    let url = format!("https://{}", host);

    println!("SmartRoute whitelist-test");
    println!("────────────────────────────────────────────────────────");
    println!("Config: {}", input.display());
    println!("Target domain: {}", host);
    println!(
        "SOCKS5: {}:{}",
        config.general.listen, config.general.listen_port
    );
    println!();

    let mut failed = 0usize;

    if check_killswitch_enabled() {
        ok("kill-switch is enabled");
    } else {
        fail("kill-switch is disabled");
        failed += 1;
    }

    if config.general.final_outbound == "direct" {
        fail("final_outbound is direct");
        failed += 1;
    } else {
        ok(&format!(
            "final_outbound is proxy/chain: {}",
            config.general.final_outbound
        ));
    }

    let direct_rules = config
        .rules
        .iter()
        .filter(|rule| rule.outbound == "direct")
        .count();

    if direct_rules == 0 {
        ok("no rules use outbound = direct");
    } else {
        fail(&format!("{} rule(s) use outbound = direct", direct_rules));
        failed += 1;
    }

    println!();
    println!("Direct connection test:");
    let direct_ok = curl_head(&url, None, 5)?;

    if direct_ok {
        fail("direct curl succeeded");
        failed += 1;
    } else {
        ok("direct curl is blocked");
    }

    println!();
    println!("SOCKS connection test:");
    let socks = format!("{}:{}", config.general.listen, config.general.listen_port);
    let socks_ok = curl_head(&url, Some(&socks), 15)?;

    if socks_ok {
        ok("SOCKS curl works through SmartRoute");
    } else {
        fail("SOCKS curl failed");
        failed += 1;
    }

    println!();
    println!("Whitelist packet test:");

    match capture_and_check(input, &host, interface) {
        Ok(result) => {
            if result.destinations.is_empty() {
                warn("no TCP SYN destinations captured");
            } else {
                println!("Captured destinations:");
                for dst in &result.destinations {
                    println!("  {}", dst);
                }
            }

            if result.sni_names.is_empty() {
                warn("no TLS SNI captured");
                failed += 1;
            } else {
                println!("Captured SNI:");
                for sni in &result.sni_names {
                    match classify_sni(sni) {
                        Some(group) => println!("  {} -> {}", sni, group),
                        None => println!("  {} -> unknown", sni),
                    }
                }
            }

            if result.unknown_destinations.is_empty() {
                ok("all captured destinations are known proxy nodes");
            } else {
                fail("captured unknown destinations");
                for dst in result.unknown_destinations {
                    println!("  unknown: {}", dst);
                }
                failed += 1;
            }

            if result.leaked_target_sni.is_empty() {
                ok("target domain was not visible in SNI");
            } else {
                fail("target domain appeared in SNI");
                for sni in result.leaked_target_sni {
                    println!("  leaked SNI: {}", sni);
                }
                failed += 1;
            }

            if result.unknown_sni.is_empty() {
                ok("all captured SNI names belong to whitelist groups");
            } else {
                fail("some captured SNI names are not in whitelist groups");
                for sni in result.unknown_sni {
                    println!("  unknown SNI: {}", sni);
                }
                failed += 1;
            }
        }
        Err(err) => {
            fail(&format!("packet test failed: {:#}", err));
            failed += 1;
        }
    }

    println!();
    println!("Result:");
    if failed == 0 {
        ok("whitelist-compatible route detected");
        Ok(())
    } else {
        anyhow::bail!("whitelist-test failed: {} problem(s) found", failed);
    }
}

struct WhitelistCaptureResult {
    sni_names: BTreeSet<String>,
    destinations: BTreeSet<String>,
    unknown_destinations: Vec<String>,
    leaked_target_sni: Vec<String>,
    unknown_sni: Vec<String>,
}

fn capture_and_check(
    input: &Path,
    host: &str,
    interface: Option<&str>,
) -> Result<WhitelistCaptureResult> {
    require_command("tcpdump")?;
    require_command("tshark")?;

    let config = load_config(input)?;

    let iface = match interface {
        Some(iface) => iface.to_string(),
        None => detect_default_interface()?,
    };

    let allowed = allowed_destinations(&config);
    if allowed.is_empty() {
        anyhow::bail!("no IP proxy nodes found in config");
    }

    let pcap = temp_pcap_path();
    let filter = build_tcpdump_filter(&config);

    println!("Interface: {}", iface);
    println!("PCAP: {}", pcap.display());

    let mut child = Command::new("tcpdump")
        .args(["-i", &iface, "-nn", "-s0", "-w"])
        .arg(&pcap)
        .arg(&filter)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to start tcpdump")?;

    thread::sleep(Duration::from_millis(800));

    let url = format!("https://{}", host);
    let socks = format!("{}:{}", config.general.listen, config.general.listen_port);
    let _ = curl_head(&url, Some(&socks), 15);

    thread::sleep(Duration::from_millis(1200));

    let _ = child.kill();
    let _ = child.wait();

    let sni_names = tshark_sni(&pcap)?;
    let destinations = tshark_destinations(&pcap)?;

    let unknown_destinations = destinations
        .iter()
        .filter(|dst| !allowed.contains(*dst))
        .cloned()
        .collect::<Vec<_>>();

    let leaked_target_sni = sni_names
        .iter()
        .filter(|sni| is_same_or_subdomain(sni, host))
        .cloned()
        .collect::<Vec<_>>();

    let unknown_sni = sni_names
        .iter()
        .filter(|sni| classify_sni(sni).is_none())
        .cloned()
        .collect::<Vec<_>>();

    Ok(WhitelistCaptureResult {
        sni_names,
        destinations,
        unknown_destinations,
        leaked_target_sni,
        unknown_sni,
    })
}

fn allowed_destinations(config: &SmartRouteConfig) -> HashSet<String> {
    let mut allowed = HashSet::new();

    for node in &config.nodes {
        if node.server.parse::<IpAddr>().is_ok() {
            allowed.insert(format!("{}\t{}", node.server, node.port));
        }
    }

    allowed
}

fn build_tcpdump_filter(config: &SmartRouteConfig) -> String {
    let mut ports = config
        .nodes
        .iter()
        .map(|node| node.port)
        .collect::<Vec<_>>();

    ports.sort();
    ports.dedup();

    if ports.is_empty() {
        return "tcp".to_string();
    }

    ports
        .iter()
        .map(|port| format!("tcp port {}", port))
        .collect::<Vec<_>>()
        .join(" or ")
}

fn tshark_sni(pcap: &Path) -> Result<BTreeSet<String>> {
    let output = Command::new("tshark")
        .arg("-r")
        .arg(pcap)
        .args([
            "-Y",
            "tls.handshake.extensions_server_name",
            "-T",
            "fields",
            "-e",
            "tls.handshake.extensions_server_name",
        ])
        .output()
        .context("failed to run tshark SNI check")?;

    let text = String::from_utf8_lossy(&output.stdout);

    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect())
}

fn tshark_destinations(pcap: &Path) -> Result<BTreeSet<String>> {
    let output = Command::new("tshark")
        .arg("-r")
        .arg(pcap)
        .args([
            "-Y",
            "tcp.flags.syn == 1 && tcp.flags.ack == 0",
            "-T",
            "fields",
            "-e",
            "ip.dst",
            "-e",
            "tcp.dstport",
        ])
        .output()
        .context("failed to run tshark destination check")?;

    let text = String::from_utf8_lossy(&output.stdout);

    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect())
}

fn curl_head(url: &str, socks: Option<&str>, max_time: u64) -> Result<bool> {
    let mut cmd = Command::new("curl");

    if let Some(socks) = socks {
        cmd.args(["--socks5-hostname", socks]);
    }

    let output = cmd
        .args([
            "-I",
            url,
            "--max-time",
            &max_time.to_string(),
            "-sS",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
        ])
        .output()
        .context("failed to run curl")?;

    if output.status.success() {
        let code = String::from_utf8_lossy(&output.stdout);
        println!("  curl {} -> HTTP {}", url, code.trim());
        Ok(true)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!("  curl {} -> blocked/failed ({})", url, stderr.trim());
        Ok(false)
    }
}

fn check_killswitch_enabled() -> bool {
    Command::new("nft")
        .args(["list", "table", "inet", "smartroute"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn detect_default_interface() -> Result<String> {
    let output = Command::new("ip")
        .args(["route", "get", "1.1.1.1"])
        .output()
        .context("failed to detect default interface")?;

    let text = String::from_utf8_lossy(&output.stdout);
    let mut prev = "";

    for part in text.split_whitespace() {
        if prev == "dev" {
            return Ok(part.to_string());
        }

        prev = part;
    }

    anyhow::bail!("could not detect default network interface");
}

fn require_command(name: &str) -> Result<()> {
    let status = Command::new(name)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("{} not found", name))?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("{} returned error", name);
    }
}

fn temp_pcap_path() -> PathBuf {
    let pid = std::process::id();
    PathBuf::from(format!("/tmp/smartroute-whitelist-test-{}.pcap", pid))
}

fn normalize_host(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(value)
        .split(':')
        .next()
        .unwrap_or(value)
        .to_string()
}

fn is_same_or_subdomain(sni: &str, host: &str) -> bool {
    let sni = sni.trim().trim_end_matches('.').to_ascii_lowercase();
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();

    sni == host || sni.ends_with(&format!(".{}", host))
}

fn classify_sni(sni: &str) -> Option<&'static str> {
    let sni = sni.trim().trim_end_matches('.').to_ascii_lowercase();

    for (group, domains) in ALLOWED_GROUPS {
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

fn ok(message: &str) {
    println!("[OK] {}", message);
}

fn warn(message: &str) {
    println!("[WARN] {}", message);
}

fn fail(message: &str) {
    println!("[FAIL] {}", message);
}
