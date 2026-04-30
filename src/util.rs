use crate::config::{Chain, LocalProfile, Node, Rule, SmartRouteConfig};
use anyhow::{Context, Result};
use std::{fs, path::Path};

pub fn sanitize_tag(input: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;

    for ch in input.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            Some(ch.to_ascii_lowercase())
        } else if ch.is_ascii_whitespace() || ch == '-' || ch == '_' || ch == '|' {
            Some('-')
        } else {
            None
        };

        if let Some(c) = mapped {
            if c == '-' {
                if !last_dash && !out.is_empty() {
                    out.push('-');
                    last_dash = true;
                }
            } else {
                out.push(c);
                last_dash = false;
            }
        }
    }

    while out.ends_with('-') {
        out.pop();
    }

    if out.is_empty() {
        "node".to_string()
    } else {
        out
    }
}

pub fn escape_toml_string(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn hex_to_utf8(input: &str) -> Option<String> {
    let cleaned = input.trim();

    if cleaned.len() % 2 != 0 {
        return None;
    }

    if !cleaned.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    let bytes: Result<Vec<u8>, _> = (0..cleaned.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&cleaned[i..i + 2], 16))
        .collect();

    let bytes = bytes.ok()?;
    String::from_utf8(bytes).ok()
}

pub fn write_config_toml(path: &Path, config: &SmartRouteConfig) -> Result<()> {
    let mut out = String::new();

    out.push_str("[general]\n");
    out.push_str(&format!(
        "mode = \"{}\"\n",
        escape_toml_string(&config.general.mode)
    ));
    out.push_str(&format!(
        "listen = \"{}\"\n",
        escape_toml_string(&config.general.listen)
    ));
    out.push_str(&format!("listen_port = {}\n", config.general.listen_port));
    out.push_str(&format!(
        "final_outbound = \"{}\"\n\n",
        escape_toml_string(&config.general.final_outbound)
    ));

    for node in &config.nodes {
        write_node_toml_from_node(&mut out, node);
    }

    for chain in &config.chains {
        write_chain_toml(&mut out, chain);
    }

    for profile in &config.local_profiles {
        write_local_profile_toml(&mut out, profile);
    }

    for rule in &config.rules {
        write_rule_toml(&mut out, rule);
    }

    fs::write(path, out).with_context(|| format!("Failed to write {}", path.display()))?;

    Ok(())
}

fn write_node_toml_from_node(out: &mut String, node: &Node) {
    out.push_str(&format!(
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
        out.push_str(&format!("uuid = \"{}\"\n", escape_toml_string(uuid)));
    }
    if let Some(flow) = &node.flow {
        out.push_str(&format!("flow = \"{}\"\n", escape_toml_string(flow)));
    }
    if let Some(security) = &node.security {
        out.push_str(&format!(
            "security = \"{}\"\n",
            escape_toml_string(security)
        ));
    }
    if let Some(server_name) = &node.server_name {
        out.push_str(&format!(
            "server_name = \"{}\"\n",
            escape_toml_string(server_name)
        ));
    }
    if let Some(pk) = &node.reality_public_key {
        out.push_str(&format!(
            "reality_public_key = \"{}\"\n",
            escape_toml_string(pk)
        ));
    }
    if let Some(sid) = &node.reality_short_id {
        out.push_str(&format!(
            "reality_short_id = \"{}\"\n",
            escape_toml_string(sid)
        ));
    }

    out.push('\n');
}

fn write_chain_toml(out: &mut String, chain: &Chain) {
    let outbounds = chain
        .outbounds
        .iter()
        .map(|item| format!("\"{}\"", escape_toml_string(item)))
        .collect::<Vec<_>>()
        .join(", ");

    out.push_str(&format!(
        r#"[[chains]]
tag = "{}"
outbounds = [{}]

"#,
        escape_toml_string(&chain.tag),
        outbounds
    ));
}

fn write_local_profile_toml(out: &mut String, profile: &LocalProfile) {
    out.push_str(&format!(
        r#"[[local_profiles]]
tag = "{}"
listen = "{}"
listen_port = {}
outbound = "{}"

"#,
        escape_toml_string(&profile.tag),
        escape_toml_string(&profile.listen),
        profile.listen_port,
        escape_toml_string(&profile.outbound)
    ));
}

fn write_rule_toml(out: &mut String, rule: &Rule) {
    out.push_str(&format!(
        r#"[[rules]]
type = "{}"
value = "{}"
outbound = "{}"

"#,
        escape_toml_string(&rule.rule_type),
        escape_toml_string(&rule.value),
        escape_toml_string(&rule.outbound)
    ));
}
