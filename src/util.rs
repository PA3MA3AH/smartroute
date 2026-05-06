use crate::config::{Chain, LocalProfile, Node, Rule, SmartRouteConfig};
use anyhow::{Context, Result};
use std::{fs, path::Path};

pub fn atomic_write(path: &Path, content: &str) -> Result<()> {
    let parent = path.parent().context("Path has no parent directory")?;

    // Create a temporary file in the same directory as the target
    let temp_path = parent.join(format!(
        ".{}.tmp.{}",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("config"),
        std::process::id()
    ));

    tracing::debug!(
        temp_file = %temp_path.display(),
        target_file = %path.display(),
        size = %content.len(),
        "Writing to temporary file"
    );

    // Write to temporary file
    fs::write(&temp_path, content)
        .with_context(|| format!("Failed to write temporary file: {}", temp_path.display()))?;

    // Atomically rename temporary file to target
    fs::rename(&temp_path, path)
        .with_context(|| format!("Failed to rename {} to {}", temp_path.display(), path.display()))?;

    tracing::debug!(
        file = %path.display(),
        "Atomic write completed successfully"
    );

    Ok(())
}

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

    crate::backup::create_backup_if_exists(path)?;

    atomic_write(path, &out)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_tag_alphanumeric() {
        assert_eq!(sanitize_tag("test123"), "test123");
        assert_eq!(sanitize_tag("TestNode"), "testnode");
    }

    #[test]
    fn test_sanitize_tag_with_spaces() {
        assert_eq!(sanitize_tag("test node"), "test-node");
        assert_eq!(sanitize_tag("test  node"), "test-node");
    }

    #[test]
    fn test_sanitize_tag_with_special_chars() {
        assert_eq!(sanitize_tag("test_node"), "test-node");
        assert_eq!(sanitize_tag("test|node"), "test-node");
        assert_eq!(sanitize_tag("test-node"), "test-node");
    }

    #[test]
    fn test_sanitize_tag_leading_trailing_dashes() {
        assert_eq!(sanitize_tag("-test-"), "test");
        assert_eq!(sanitize_tag("--test--"), "test");
    }

    #[test]
    fn test_sanitize_tag_empty() {
        assert_eq!(sanitize_tag(""), "node");
        assert_eq!(sanitize_tag("---"), "node");
        assert_eq!(sanitize_tag("   "), "node");
    }

    #[test]
    fn test_sanitize_tag_unicode() {
        assert_eq!(sanitize_tag("тест"), "node");
        assert_eq!(sanitize_tag("test🚀node"), "testnode");
    }

    #[test]
    fn test_escape_toml_string_no_escape() {
        assert_eq!(escape_toml_string("simple"), "simple");
        assert_eq!(escape_toml_string("test-node"), "test-node");
    }

    #[test]
    fn test_escape_toml_string_with_quotes() {
        assert_eq!(escape_toml_string(r#"test"value"#), r#"test\"value"#);
        assert_eq!(escape_toml_string(r#""quoted""#), r#"\"quoted\""#);
    }

    #[test]
    fn test_escape_toml_string_with_backslash() {
        assert_eq!(escape_toml_string(r"test\value"), r"test\\value");
        assert_eq!(escape_toml_string(r"C:\path\to\file"), r"C:\\path\\to\\file");
    }

    #[test]
    fn test_escape_toml_string_combined() {
        assert_eq!(escape_toml_string(r#"test\"value"#), r#"test\\\"value"#);
    }

    #[test]
    fn test_hex_to_utf8_valid() {
        assert_eq!(hex_to_utf8("48656c6c6f"), Some("Hello".to_string()));
        assert_eq!(hex_to_utf8("776f726c64"), Some("world".to_string()));
    }

    #[test]
    fn test_hex_to_utf8_uppercase() {
        assert_eq!(hex_to_utf8("48656C6C6F"), Some("Hello".to_string()));
        assert_eq!(hex_to_utf8("48656c6C6f"), Some("Hello".to_string()));
    }

    #[test]
    fn test_hex_to_utf8_with_whitespace() {
        assert_eq!(hex_to_utf8("  48656c6c6f  "), Some("Hello".to_string()));
    }

    #[test]
    fn test_hex_to_utf8_odd_length() {
        assert_eq!(hex_to_utf8("123"), None);
        assert_eq!(hex_to_utf8("12345"), None);
    }

    #[test]
    fn test_hex_to_utf8_invalid_chars() {
        assert_eq!(hex_to_utf8("48656g6c6f"), None);
        assert_eq!(hex_to_utf8("xyz"), None);
    }

    #[test]
    fn test_hex_to_utf8_invalid_utf8() {
        assert_eq!(hex_to_utf8("c328"), None);
        assert_eq!(hex_to_utf8("a0a1"), None);
    }

    #[test]
    fn test_hex_to_utf8_empty() {
        assert_eq!(hex_to_utf8(""), Some("".to_string()));
        assert_eq!(hex_to_utf8("   "), Some("".to_string()));
    }

    #[test]
    fn test_write_config_toml_minimal() {
        use crate::config::{General, SmartRouteConfig, SubscriptionSettings};
        use tempfile::NamedTempFile;

        let config = SmartRouteConfig {
            general: General {
                mode: "socks".to_string(),
                listen: "127.0.0.1".to_string(),
                listen_port: 1081,
                final_outbound: "direct".to_string(),
            },
            subscription: SubscriptionSettings::default(),
            nodes: vec![],
            chains: vec![],
            local_profiles: vec![],
            rules: vec![],
        };

        let temp_file = NamedTempFile::new().unwrap();
        write_config_toml(temp_file.path(), &config).unwrap();

        let content = std::fs::read_to_string(temp_file.path()).unwrap();
        assert!(content.contains("[general]"));
        assert!(content.contains("mode = \"socks\""));
        assert!(content.contains("listen_port = 1081"));
        assert!(content.contains("final_outbound = \"direct\""));
    }

    #[test]
    fn test_write_config_toml_with_nodes() {
        use crate::config::{General, Node, SmartRouteConfig, SubscriptionSettings};
        use tempfile::NamedTempFile;

        let config = SmartRouteConfig {
            general: General {
                mode: "socks".to_string(),
                listen: "127.0.0.1".to_string(),
                listen_port: 1081,
                final_outbound: "node1".to_string(),
            },
            subscription: SubscriptionSettings::default(),
            nodes: vec![Node {
                tag: "node1".to_string(),
                node_type: "vless".to_string(),
                server: "example.com".to_string(),
                port: 443,
                uuid: Some("test-uuid".to_string()),
                flow: None,
                security: Some("reality".to_string()),
                server_name: Some("example.com".to_string()),
                utls_fingerprint: Some("chrome".to_string()),
                reality_public_key: Some("test-key".to_string()),
                reality_short_id: Some("test-id".to_string()),
            }],
            chains: vec![],
            local_profiles: vec![],
            rules: vec![],
        };

        let temp_file = NamedTempFile::new().unwrap();
        write_config_toml(temp_file.path(), &config).unwrap();

        let content = std::fs::read_to_string(temp_file.path()).unwrap();
        assert!(content.contains("[[nodes]]"));
        assert!(content.contains("tag = \"node1\""));
        assert!(content.contains("type = \"vless\""));
        assert!(content.contains("server = \"example.com\""));
        assert!(content.contains("uuid = \"test-uuid\""));
    }

    #[test]
    fn test_atomic_write_creates_file() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        let content = "test content";

        atomic_write(&file_path, content).unwrap();

        let read_content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(read_content, content);
    }

    #[test]
    fn test_atomic_write_overwrites_existing() {
        use tempfile::NamedTempFile;

        let temp_file = NamedTempFile::new().unwrap();
        std::fs::write(temp_file.path(), "old content").unwrap();

        let new_content = "new content";
        atomic_write(temp_file.path(), new_content).unwrap();

        let read_content = std::fs::read_to_string(temp_file.path()).unwrap();
        assert_eq!(read_content, new_content);
    }

    #[test]
    fn test_atomic_write_no_partial_writes() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");

        // Write large content
        let content = "x".repeat(10000);
        atomic_write(&file_path, &content).unwrap();

        // File should either exist with full content or not exist at all
        let read_content = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(read_content.len(), content.len());
    }
}
