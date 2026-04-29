use crate::util::{escape_toml_string, hex_to_utf8, sanitize_tag};
use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine as _};
use std::{collections::HashSet, fs, path::Path};

#[derive(Debug)]
struct ParsedNode {
    tag: String,
    node_type: String,
    server: String,
    port: u16,
    uuid: Option<String>,
    flow: Option<String>,
    security: Option<String>,
    server_name: Option<String>,
    reality_public_key: Option<String>,
    reality_short_id: Option<String>,
}

pub fn import_url(url: &str, output: &Path) -> Result<()> {
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
        }

        if line.starts_with("vless://") {
            if let Some(node) = parse_vless(line) {
                push_unique_node(&mut nodes, &mut used_tags, node);
            }
        }
    }

    if nodes.is_empty() {
        anyhow::bail!("No supported nodes found in subscription. Supported: ss://, vless://");
    }

    let count = nodes.len();

    let mut toml = String::from(
        r#"[general]
mode = "socks"
listen = "127.0.0.1"
listen_port = 1081
final_outbound = "direct"

"#,
    );

    for node in nodes {
        write_node_toml(&mut toml, &node);
    }

    fs::write(output, toml).context("Failed to write output file")?;

    println!("Imported {} nodes -> {}", count, output.display());

    Ok(())
}

fn push_unique_node(nodes: &mut Vec<ParsedNode>, used_tags: &mut HashSet<String>, mut node: ParsedNode) {
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
    if let Some(flow) = &node.flow {
        toml.push_str(&format!("flow = \"{}\"\n", escape_toml_string(flow)));
    }
    if let Some(security) = &node.security {
        toml.push_str(&format!("security = \"{}\"\n", escape_toml_string(security)));
    }
    if let Some(server_name) = &node.server_name {
        toml.push_str(&format!("server_name = \"{}\"\n", escape_toml_string(server_name)));
    }
    if let Some(pk) = &node.reality_public_key {
        toml.push_str(&format!(
            "reality_public_key = \"{}\"\n",
            escape_toml_string(pk)
        ));
    }
    if let Some(sid) = &node.reality_short_id {
        toml.push_str(&format!(
            "reality_short_id = \"{}\"\n",
            escape_toml_string(sid)
        ));
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
    let raw = link.strip_prefix("ss://")?;
    let without_fragment = raw.split('#').next().unwrap_or(raw);

    let decoded = if !without_fragment.contains('@') {
        let data = general_purpose::STANDARD
            .decode(without_fragment)
            .or_else(|_| general_purpose::URL_SAFE_NO_PAD.decode(without_fragment))
            .ok()?;

        String::from_utf8(data).ok()?
    } else {
        without_fragment.to_string()
    };

    let (_, host_port) = decoded.split_once('@')?;
    let (server, port_raw) = host_port.rsplit_once(':')?;
    let port: u16 = port_raw.parse().ok()?;

    Some(ParsedNode {
        tag: format!("ss-{}", sanitize_tag(server)),
        node_type: "socks".to_string(),
        server: server.to_string(),
        port,
        uuid: None,
        flow: None,
        security: None,
        server_name: None,
        reality_public_key: None,
        reality_short_id: None,
    })
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
    let mut reality_public_key = None;
    let mut reality_short_id = None;

    for (key, value) in parsed.query_pairs() {
        match key.as_ref() {
            "flow" => flow = Some(value.to_string()),
            "security" => security = Some(value.to_string()),
            "sni" => server_name = Some(value.to_string()),
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
        reality_public_key,
        reality_short_id,
    })
}
