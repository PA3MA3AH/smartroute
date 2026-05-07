use crate::{
    config::{Node, load_config, save_config},
    resolve::resolve_domains_to_ip,
    util::{escape_toml_string, hex_to_utf8, sanitize_tag},
};

use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose};
use serde_json;
use std::{collections::HashSet, fs, path::Path};

#[derive(Debug)]
struct ParsedNode {
    tag: String,
    node_type: String,
    server: String,
    port: u16,
    uuid: Option<String>,
    alter_id: Option<u32>,
    flow: Option<String>,
    method: Option<String>,
    password: Option<String>,
    security: Option<String>,
    server_name: Option<String>,
    utls_fingerprint: Option<String>,
    reality_public_key: Option<String>,
    reality_short_id: Option<String>,
    up_mbps: Option<u32>,
    down_mbps: Option<u32>,
    obfs: Option<String>,
    obfs_password: Option<String>,
}

pub fn import_url(url: &str, output: &Path) -> Result<()> {
    let nodes = download_nodes(url)?;

    let count = nodes.len();

    let mut toml = format!(
        r#"[general]
mode = "socks"
listen = "127.0.0.1"
listen_port = 1081
final_outbound = "direct"

[subscription]
url = "{}"
auto_refresh = 3600

"#,
        escape_toml_string(url)
    );

    for node in nodes {
        write_node_toml(&mut toml, &node);
    }

    crate::backup::create_backup_if_exists(output)?;

    fs::write(output, toml).context("Failed to write output file")?;

    let resolved = resolve_domains_to_ip(output)?;

    println!("Imported {} nodes -> {}", count, output.display());
    println!("Resolved {} domain node(s) to IP", resolved);

    Ok(())
}

pub fn refresh_config_nodes_from_subscription(config_path: &Path) -> Result<usize> {
    let mut config = load_config(config_path)?;

    let Some(url) = config
        .subscription
        .url
        .clone()
        .filter(|url| !url.trim().is_empty())
    else {
        println!("Subscription refresh skipped: no subscription.url in config");
        return Ok(0);
    };

    let parsed_nodes = download_nodes(&url)?;
    let count = parsed_nodes.len();

    config.nodes = parsed_nodes.into_iter().map(parsed_node_to_node).collect();

    save_config(config_path, &config)?;
    let resolved = resolve_domains_to_ip(config_path)?;

    println!(
        "Subscription refreshed: {} nodes, {} resolved to IP",
        count, resolved
    );

    Ok(count)
}

fn download_nodes(url: &str) -> Result<Vec<ParsedNode>> {
    println!("Downloading subscription...");

    let resp = reqwest::blocking::get(url)
        .context("Failed to download subscription")?
        .text()
        .context("Failed to read response")?;

    let decoded = decode_subscription_body(&resp);

    let mut nodes = Vec::new();
    let mut used_tags = HashSet::new();

    for line in decoded.lines() {
        let line = line.trim();

        if line.is_empty() {
            continue;
        }

        if line.starts_with("ss://") {
            if let Some(node) = parse_ss(line) {
                push_unique_node(&mut nodes, &mut used_tags, node);
            }
        } else if line.starts_with("vless://") {
            if let Some(node) = parse_vless(line) {
                push_unique_node(&mut nodes, &mut used_tags, node);
            }
        } else if line.starts_with("trojan://") {
            if let Some(node) = parse_trojan(line) {
                push_unique_node(&mut nodes, &mut used_tags, node);
            }
        } else if line.starts_with("vmess://") {
            if let Some(node) = parse_vmess(line) {
                push_unique_node(&mut nodes, &mut used_tags, node);
            }
        } else if line.starts_with("hysteria2://") || line.starts_with("hy2://") {
            if let Some(node) = parse_hysteria2(line) {
                push_unique_node(&mut nodes, &mut used_tags, node);
            }
        }
    }

    if nodes.is_empty() {
        anyhow::bail!("No supported nodes found in subscription. Supported: ss://, vless://");
    }

    Ok(nodes)
}

fn parsed_node_to_node(node: ParsedNode) -> Node {
    Node {
        tag: node.tag,
        node_type: node.node_type,
        server: node.server,
        port: node.port,
        uuid: node.uuid,
        flow: node.flow,
        security: node.security,
        server_name: node.server_name,
        utls_fingerprint: node.utls_fingerprint,
        reality_public_key: node.reality_public_key,
        reality_short_id: node.reality_short_id,
        method: node.method,
        password: node.password,
        alter_id: node.alter_id,
        up_mbps: node.up_mbps,
        down_mbps: node.down_mbps,
        obfs: node.obfs,
        obfs_password: node.obfs_password,
    }
}

fn push_unique_node(
    nodes: &mut Vec<ParsedNode>,
    used_tags: &mut HashSet<String>,
    mut node: ParsedNode,
) {
    let base = node.tag.clone();
    let mut tag = base.clone();
    let mut i = 1;

    while used_tags.contains(&tag) {
        tag = format!("{}-{}", base, i);
        i += 1;
    }

    node.tag = tag.clone();
    used_tags.insert(tag);
    nodes.push(node);
}

fn write_node_toml(toml: &mut String, node: &ParsedNode) {
    toml.push_str(&format!(
        r#"[[nodes]]
tag = "{}"
type = "{}"
server = "{}"
port = {}
"#,
        escape_toml_string(&node.tag),
        escape_toml_string(&node.node_type),
        escape_toml_string(&node.server),
        node.port
    ));

    if let Some(uuid) = &node.uuid {
        toml.push_str(&format!("uuid = \"{}\"\n", escape_toml_string(uuid)));
    }
    if let Some(alter_id) = node.alter_id {
        toml.push_str(&format!("alter_id = {}\n", alter_id));
    }
    if let Some(flow) = &node.flow {
        toml.push_str(&format!("flow = \"{}\"\n", escape_toml_string(flow)));
    }
    if let Some(method) = &node.method {
        toml.push_str(&format!("method = \"{}\"\n", escape_toml_string(method)));
    }
    if let Some(password) = &node.password {
        toml.push_str(&format!("password = \"{}\"\n", escape_toml_string(password)));
    }
    if let Some(security) = &node.security {
        toml.push_str(&format!("security = \"{}\"\n", escape_toml_string(security)));
    }
    if let Some(server_name) = &node.server_name {
        toml.push_str(&format!("server_name = \"{}\"\n", escape_toml_string(server_name)));
    }
    if let Some(fingerprint) = &node.utls_fingerprint {
        toml.push_str(&format!("utls_fingerprint = \"{}\"\n", escape_toml_string(fingerprint)));
    }
    if let Some(pk) = &node.reality_public_key {
        toml.push_str(&format!("reality_public_key = \"{}\"\n", escape_toml_string(pk)));
    }
    if let Some(sid) = &node.reality_short_id {
        toml.push_str(&format!("reality_short_id = \"{}\"\n", escape_toml_string(sid)));
    }
    if let Some(up) = node.up_mbps {
        toml.push_str(&format!("up_mbps = {}\n", up));
    }
    if let Some(down) = node.down_mbps {
        toml.push_str(&format!("down_mbps = {}\n", down));
    }
    if let Some(obfs) = &node.obfs {
        toml.push_str(&format!("obfs = \"{}\"\n", escape_toml_string(obfs)));
    }
    if let Some(obfs_password) = &node.obfs_password {
        toml.push_str(&format!("obfs_password = \"{}\"\n", escape_toml_string(obfs_password)));
    }

    toml.push('\n');
}

fn decode_subscription_body(body: &str) -> String {
    let trimmed = body.trim();

    if trimmed.contains("://") {
        return trimmed.to_string();
    }

    let normalized = trimmed.replace('\n', "").replace('\r', "");

    if let Ok(data) = general_purpose::STANDARD.decode(&normalized) {
        if let Ok(text) = String::from_utf8(data) {
            return text;
        }
    }

    if let Ok(data) = general_purpose::URL_SAFE_NO_PAD.decode(&normalized) {
        if let Ok(text) = String::from_utf8(data) {
            return text;
        }
    }

    trimmed.to_string()
}

fn parse_ss(link: &str) -> Option<ParsedNode> {
    // ss://BASE64(method:password)@host:port#tag
    // ss://BASE64(method:password@host:port)#tag  (legacy)
    let raw = link.strip_prefix("ss://")?;

    let (without_fragment, fragment) = match raw.split_once('#') {
        Some((a, b)) => (a, b),
        None => (raw, ""),
    };

    let tag_raw = urlencoding::decode(fragment)
        .unwrap_or_else(|_| fragment.into())
        .to_string();
    let tag_base = if tag_raw.is_empty() {
        "ss".to_string()
    } else {
        sanitize_tag(&tag_raw)
    };

    // Modern format: BASE64(method:password)@host:port
    if let Some(at_pos) = without_fragment.rfind('@') {
        let userinfo_b64 = &without_fragment[..at_pos];
        let hostport = &without_fragment[at_pos + 1..];

        let userinfo = general_purpose::STANDARD
            .decode(userinfo_b64)
            .or_else(|_| general_purpose::URL_SAFE_NO_PAD.decode(userinfo_b64))
            .ok()
            .and_then(|b| String::from_utf8(b).ok())
            .unwrap_or_else(|| userinfo_b64.to_string());

        let (method, password) = userinfo.split_once(':')?;
        let (server, port_raw) = hostport.rsplit_once(':')?;
        let port: u16 = port_raw.parse().ok()?;

        return Some(ParsedNode {
            tag: format!("{}-{}", tag_base, sanitize_tag(server)),
            node_type: "shadowsocks".to_string(),
            server: server.to_string(),
            port,
            method: Some(method.to_string()),
            password: Some(password.to_string()),
            uuid: None,
            alter_id: None,
            flow: None,
            security: None,
            server_name: None,
            utls_fingerprint: None,
            reality_public_key: None,
            reality_short_id: None,
            up_mbps: None,
            down_mbps: None,
            obfs: None,
            obfs_password: None,
        });
    }

    // Legacy format: BASE64(method:password@host:port)
    let decoded = general_purpose::STANDARD
        .decode(without_fragment)
        .or_else(|_| general_purpose::URL_SAFE_NO_PAD.decode(without_fragment))
        .ok()
        .and_then(|b| String::from_utf8(b).ok())?;

    let (userinfo, hostport) = decoded.split_once('@')?;
    let (method, password) = userinfo.split_once(':')?;
    let (server, port_raw) = hostport.rsplit_once(':')?;
    let port: u16 = port_raw.parse().ok()?;

    Some(ParsedNode {
        tag: format!("{}-{}", tag_base, sanitize_tag(server)),
        node_type: "shadowsocks".to_string(),
        server: server.to_string(),
        port,
        method: Some(method.to_string()),
        password: Some(password.to_string()),
        uuid: None,
        alter_id: None,
        flow: None,
        security: None,
        server_name: None,
        utls_fingerprint: None,
        reality_public_key: None,
        reality_short_id: None,
        up_mbps: None,
        down_mbps: None,
        obfs: None,
        obfs_password: None,
    })
}

fn parse_trojan(link: &str) -> Option<ParsedNode> {
    // trojan://password@host:port?sni=...#tag
    let parsed = url::Url::parse(link).ok()?;

    let password = urlencoding::decode(parsed.username()).ok()?.to_string();
    if password.is_empty() {
        return None;
    }

    let server = parsed.host_str()?.to_string();
    let port = parsed.port().unwrap_or(443);

    let mut server_name = None;
    let mut utls_fingerprint = None;

    for (key, value) in parsed.query_pairs() {
        match key.as_ref() {
            "sni" | "peer" => server_name = Some(value.to_string()),
            "fp" | "fingerprint" => utls_fingerprint = Some(value.to_string()),
            _ => {}
        }
    }

    let tag = tag_from_fragment(parsed.fragment(), &server, "trojan");

    Some(ParsedNode {
        tag,
        node_type: "trojan".to_string(),
        server,
        port,
        password: Some(password),
        server_name,
        utls_fingerprint,
        uuid: None,
        alter_id: None,
        flow: None,
        method: None,
        security: Some("tls".to_string()),
        reality_public_key: None,
        reality_short_id: None,
        up_mbps: None,
        down_mbps: None,
        obfs: None,
        obfs_password: None,
    })
}

fn parse_vmess(link: &str) -> Option<ParsedNode> {
    // vmess://BASE64(json)
    let b64 = link.strip_prefix("vmess://")?;
    let json_bytes = general_purpose::STANDARD
        .decode(b64)
        .or_else(|_| general_purpose::URL_SAFE_NO_PAD.decode(b64))
        .ok()?;
    let json_str = String::from_utf8(json_bytes).ok()?;
    let v: serde_json::Value = serde_json::from_str(&json_str).ok()?;

    let server = v["add"].as_str()?.to_string();
    let port: u16 = v["port"]
        .as_u64()
        .or_else(|| v["port"].as_str().and_then(|s| s.parse().ok()))?
        as u16;
    let uuid = v["id"].as_str()?.to_string();
    let alter_id: u32 = v["aid"]
        .as_u64()
        .or_else(|| v["aid"].as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(0) as u32;

    let server_name = v["sni"]
        .as_str()
        .or_else(|| v["host"].as_str())
        .map(str::to_string);

    let security = match v["tls"].as_str() {
        Some("tls") => Some("tls".to_string()),
        _ => None,
    };

    let raw_name = v["ps"].as_str().unwrap_or("");
    let tag = if raw_name.is_empty() {
        format!("vmess-{}", sanitize_tag(&server))
    } else {
        sanitize_tag(raw_name)
    };

    Some(ParsedNode {
        tag,
        node_type: "vmess".to_string(),
        server,
        port,
        uuid: Some(uuid),
        alter_id: Some(alter_id),
        security,
        server_name,
        flow: None,
        method: None,
        password: None,
        utls_fingerprint: None,
        reality_public_key: None,
        reality_short_id: None,
        up_mbps: None,
        down_mbps: None,
        obfs: None,
        obfs_password: None,
    })
}

fn parse_hysteria2(link: &str) -> Option<ParsedNode> {
    // hysteria2://password@host:port?sni=...&obfs=...&obfs-password=...#tag
    let link = if link.starts_with("hy2://") {
        link.replacen("hy2://", "hysteria2://", 1)
    } else {
        link.to_string()
    };

    let parsed = url::Url::parse(&link).ok()?;

    let password = urlencoding::decode(parsed.username()).ok()?.to_string();
    if password.is_empty() {
        return None;
    }

    let server = parsed.host_str()?.to_string();
    let port = parsed.port().unwrap_or(443);

    let mut server_name = None;
    let mut up_mbps = None;
    let mut down_mbps = None;
    let mut obfs = None;
    let mut obfs_password = None;

    for (key, value) in parsed.query_pairs() {
        match key.as_ref() {
            "sni" => server_name = Some(value.to_string()),
            "upmbps" | "up" => up_mbps = value.parse().ok(),
            "downmbps" | "down" => down_mbps = value.parse().ok(),
            "obfs" => obfs = Some(value.to_string()),
            "obfs-password" => obfs_password = Some(value.to_string()),
            _ => {}
        }
    }

    let tag = tag_from_fragment(parsed.fragment(), &server, "hy2");

    Some(ParsedNode {
        tag,
        node_type: "hysteria2".to_string(),
        server,
        port,
        password: Some(password),
        server_name,
        up_mbps,
        down_mbps,
        obfs,
        obfs_password,
        uuid: None,
        alter_id: None,
        flow: None,
        method: None,
        security: None,
        utls_fingerprint: None,
        reality_public_key: None,
        reality_short_id: None,
    })
}

fn tag_from_fragment(fragment: Option<&str>, server: &str, prefix: &str) -> String {
    let raw = fragment.unwrap_or("");
    let decoded = urlencoding::decode(raw)
        .unwrap_or_else(|_| raw.into())
        .to_string();
    let maybe_utf8 = hex_to_utf8(&decoded).unwrap_or(decoded);
    let clean = maybe_utf8
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || c.is_ascii_whitespace() || *c == '-')
        .collect::<String>()
        .trim()
        .to_string();

    if clean.is_empty() {
        format!("{}-{}", prefix, sanitize_tag(server))
    } else {
        sanitize_tag(&clean)
    }
}

fn parse_vless(link: &str) -> Option<ParsedNode> {
    let parsed = url::Url::parse(link).ok()?;

    let uuid = parsed.username().to_string();
    if uuid.is_empty() {
        return None;
    }

    let server = parsed.host_str()?.to_string();
    let port = parsed.port().unwrap_or(443);

    let mut flow = None;
    let mut security = None;
    let mut server_name = None;
    let mut utls_fingerprint = None;
    let mut reality_public_key = None;
    let mut reality_short_id = None;

    for (key, value) in parsed.query_pairs() {
        match key.as_ref() {
            "flow" => flow = Some(value.to_string()),
            "security" => security = Some(value.to_string()),
            "sni" => server_name = Some(value.to_string()),

            // uTLS fingerprint:
            // vless://...?fp=chrome
            // vless://...?fingerprint=chrome
            "fp" | "fingerprint" => utls_fingerprint = Some(value.to_string()),

            "pbk" => reality_public_key = Some(value.to_string()),
            "sid" => reality_short_id = Some(value.to_string()),
            _ => {}
        }
    }

    let raw_fragment = parsed.fragment().unwrap_or("vless");

    let decoded_fragment = urlencoding::decode(raw_fragment)
        .unwrap_or_else(|_| raw_fragment.into())
        .to_string();

    let maybe_utf8 = hex_to_utf8(&decoded_fragment).unwrap_or(decoded_fragment);

    let clean_name = maybe_utf8
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || c.is_ascii_whitespace() || *c == '-')
        .collect::<String>()
        .trim()
        .replace("  ", " ");

    let tag = if clean_name.is_empty() {
        format!("vless-{}", sanitize_tag(&server))
    } else {
        sanitize_tag(&clean_name)
    };

    Some(ParsedNode {
        tag,
        node_type: "vless".to_string(),
        server,
        port,
        uuid: Some(uuid),
        flow,
        security,
        server_name,
        utls_fingerprint,
        reality_public_key,
        reality_short_id,
        method: None,
        password: None,
        alter_id: None,
        up_mbps: None,
        down_mbps: None,
        obfs: None,
        obfs_password: None,
    })
}
