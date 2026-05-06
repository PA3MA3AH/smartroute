use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fs, path::Path};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SmartRouteConfig {
    pub general: General,

    #[serde(default)]
    pub subscription: SubscriptionSettings,

    #[serde(default)]
    pub nodes: Vec<Node>,

    #[serde(default)]
    pub chains: Vec<Chain>,

    #[serde(default)]
    pub local_profiles: Vec<LocalProfile>,

    #[serde(default)]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SubscriptionSettings {
    #[serde(default)]
    pub url: Option<String>,

    #[serde(default = "default_auto_refresh")]
    pub auto_refresh: u64,
}

impl Default for SubscriptionSettings {
    fn default() -> Self {
        Self {
            url: None,
            auto_refresh: default_auto_refresh(),
        }
    }
}

fn default_auto_refresh() -> u64 {
    3600
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct General {
    #[serde(default = "default_mode")]
    pub mode: String,

    #[serde(default = "default_listen")]
    pub listen: String,

    #[serde(default = "default_listen_port")]
    pub listen_port: u16,

    pub final_outbound: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Node {
    pub tag: String,

    #[serde(rename = "type")]
    pub node_type: String,

    pub server: String,
    pub port: u16,

    #[serde(default)]
    pub uuid: Option<String>,

    #[serde(default)]
    pub flow: Option<String>,

    #[serde(default)]
    pub security: Option<String>,

    #[serde(default)]
    pub server_name: Option<String>,

    #[serde(default)]
    pub utls_fingerprint: Option<String>,

    #[serde(default)]
    pub reality_public_key: Option<String>,

    #[serde(default)]
    pub reality_short_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Chain {
    pub tag: String,

    /// Ordered list of outbound tags.
    /// Example: ["proxy-a", "proxy-b"] means: app -> SmartRoute -> proxy-a -> proxy-b -> site.
    pub outbounds: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LocalProfile {
    pub tag: String,

    #[serde(default = "default_listen")]
    pub listen: String,

    pub listen_port: u16,

    pub outbound: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Rule {
    #[serde(rename = "type")]
    pub rule_type: String,

    pub value: String,
    pub outbound: String,
}

fn default_mode() -> String {
    "socks".to_string()
}

fn default_listen() -> String {
    "127.0.0.1".to_string()
}

fn default_listen_port() -> u16 {
    1081
}

pub fn load_config(path: &Path) -> Result<SmartRouteConfig> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config: {}", path.display()))?;

    let config: SmartRouteConfig =
        toml::from_str(&raw).context("Failed to parse SmartRoute TOML config")?;

    Ok(config)
}

pub fn validate_config(config: &SmartRouteConfig) -> Result<()> {
    let mut outbounds = HashSet::new();
    let mut inbound_ports = HashSet::new();

    outbounds.insert("direct".to_string());
    outbounds.insert("block".to_string());

    match config.general.mode.as_str() {
        "socks" | "tun" => {}
        other => anyhow::bail!("Unsupported mode: {}. Supported: socks, tun", other),
    }

    if config.general.listen.trim().is_empty() {
        anyhow::bail!("general.listen cannot be empty");
    }

    inbound_ports.insert(config.general.listen_port);

    for node in &config.nodes {
        if node.tag.trim().is_empty() {
            anyhow::bail!("Node tag cannot be empty");
        }

        if outbounds.contains(&node.tag) {
            anyhow::bail!("Duplicate outbound tag: {}", node.tag);
        }

        match node.node_type.as_str() {
            "socks" => {}
            "vless" => {
                if node.uuid.as_deref().unwrap_or("").is_empty() {
                    anyhow::bail!("VLESS node {} has no uuid", node.tag);
                }
            }
            other => anyhow::bail!("Unsupported node type: {}", other),
        }

        outbounds.insert(node.tag.clone());
    }

    let base_outbounds = outbounds.clone();

    for chain in &config.chains {
        if chain.tag.trim().is_empty() {
            anyhow::bail!("Chain tag cannot be empty");
        }

        if outbounds.contains(&chain.tag) {
            anyhow::bail!("Duplicate outbound/chain tag: {}", chain.tag);
        }

        if chain.outbounds.len() < 2 {
            anyhow::bail!("Chain {} must contain at least 2 outbounds", chain.tag);
        }

        for member in &chain.outbounds {
            if member == &chain.tag {
                anyhow::bail!("Chain {} cannot reference itself", chain.tag);
            }

            if !base_outbounds.contains(member) {
                anyhow::bail!(
                    "Chain {} references unknown base outbound: {}",
                    chain.tag,
                    member
                );
            }
        }

        outbounds.insert(chain.tag.clone());
    }

    if !outbounds.contains(&config.general.final_outbound) {
        anyhow::bail!(
            "final_outbound points to unknown outbound: {}",
            config.general.final_outbound
        );
    }

    for profile in &config.local_profiles {
        if profile.tag.trim().is_empty() {
            anyhow::bail!("Local profile tag cannot be empty");
        }

        if profile.listen.trim().is_empty() {
            anyhow::bail!("Local profile {} listen cannot be empty", profile.tag);
        }

        if !inbound_ports.insert(profile.listen_port) {
            anyhow::bail!(
                "Duplicate local listen port: {}. Every profile needs its own port.",
                profile.listen_port
            );
        }

        if !outbounds.contains(&profile.outbound) {
            anyhow::bail!(
                "Local profile {} points to unknown outbound: {}",
                profile.tag,
                profile.outbound
            );
        }
    }

    for rule in &config.rules {
        match rule.rule_type.as_str() {
            "domain" | "domain_suffix" | "domain_keyword" => {}
            other => anyhow::bail!("Unsupported rule type: {}", other),
        }

        if rule.value.trim().is_empty() {
            anyhow::bail!("Rule value cannot be empty");
        }

        if !outbounds.contains(&rule.outbound) {
            anyhow::bail!(
                "Rule {} {} points to unknown outbound: {}",
                rule.rule_type,
                rule.value,
                rule.outbound
            );
        }
    }

    Ok(())
}

pub fn save_config(path: &Path, config: &SmartRouteConfig) -> Result<()> {
    let raw =
        toml::to_string_pretty(config).context("Failed to serialize SmartRoute TOML config")?;

    crate::backup::create_backup_if_exists(path)?;

    fs::write(path, raw).with_context(|| format!("Failed to write config: {}", path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_minimal_config() -> SmartRouteConfig {
        SmartRouteConfig {
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
        }
    }

    fn create_test_node(tag: &str) -> Node {
        Node {
            tag: tag.to_string(),
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
        }
    }

    #[test]
    fn test_validate_minimal_config() {
        let config = create_minimal_config();
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_validate_unsupported_mode() {
        let mut config = create_minimal_config();
        config.general.mode = "invalid".to_string();

        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unsupported mode"));
    }

    #[test]
    fn test_validate_empty_listen() {
        let mut config = create_minimal_config();
        config.general.listen = "".to_string();

        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("listen cannot be empty"));
    }

    #[test]
    fn test_validate_duplicate_node_tags() {
        let mut config = create_minimal_config();
        config.nodes = vec![
            create_test_node("node1"),
            create_test_node("node1"),
        ];
        config.general.final_outbound = "node1".to_string();

        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Duplicate outbound tag"));
    }

    #[test]
    fn test_validate_vless_without_uuid() {
        let mut config = create_minimal_config();
        let mut node = create_test_node("node1");
        node.uuid = None;
        config.nodes = vec![node];
        config.general.final_outbound = "node1".to_string();

        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("has no uuid"));
    }

    #[test]
    fn test_validate_chain_with_valid_outbounds() {
        let mut config = create_minimal_config();
        config.nodes = vec![
            create_test_node("node1"),
            create_test_node("node2"),
        ];
        config.chains = vec![Chain {
            tag: "chain1".to_string(),
            outbounds: vec!["node1".to_string(), "node2".to_string()],
        }];
        config.general.final_outbound = "chain1".to_string();

        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn test_validate_chain_with_less_than_two_outbounds() {
        let mut config = create_minimal_config();
        config.nodes = vec![create_test_node("node1")];
        config.chains = vec![Chain {
            tag: "chain1".to_string(),
            outbounds: vec!["node1".to_string()],
        }];
        config.general.final_outbound = "chain1".to_string();

        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must contain at least 2 outbounds"));
    }

    #[test]
    fn test_validate_chain_self_reference() {
        let mut config = create_minimal_config();
        config.nodes = vec![create_test_node("node1")];
        config.chains = vec![Chain {
            tag: "chain1".to_string(),
            outbounds: vec!["chain1".to_string(), "node1".to_string()],
        }];
        config.general.final_outbound = "chain1".to_string();

        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot reference itself"));
    }

    #[test]
    fn test_validate_chain_unknown_outbound() {
        let mut config = create_minimal_config();
        config.nodes = vec![create_test_node("node1")];
        config.chains = vec![Chain {
            tag: "chain1".to_string(),
            outbounds: vec!["node1".to_string(), "unknown".to_string()],
        }];
        config.general.final_outbound = "chain1".to_string();

        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown base outbound"));
    }

    #[test]
    fn test_validate_unknown_final_outbound() {
        let mut config = create_minimal_config();
        config.general.final_outbound = "unknown".to_string();

        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("final_outbound points to unknown outbound"));
    }

    #[test]
    fn test_validate_duplicate_local_profile_ports() {
        let mut config = create_minimal_config();
        config.nodes = vec![create_test_node("node1")];
        config.local_profiles = vec![
            LocalProfile {
                tag: "profile1".to_string(),
                listen: "127.0.0.1".to_string(),
                listen_port: 1082,
                outbound: "node1".to_string(),
            },
            LocalProfile {
                tag: "profile2".to_string(),
                listen: "127.0.0.1".to_string(),
                listen_port: 1082,
                outbound: "node1".to_string(),
            },
        ];
        config.general.final_outbound = "node1".to_string();

        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Duplicate local listen port"));
    }

    #[test]
    fn test_validate_local_profile_unknown_outbound() {
        let mut config = create_minimal_config();
        config.local_profiles = vec![LocalProfile {
            tag: "profile1".to_string(),
            listen: "127.0.0.1".to_string(),
            listen_port: 1082,
            outbound: "unknown".to_string(),
        }];

        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("points to unknown outbound"));
    }

    #[test]
    fn test_validate_rule_unknown_outbound() {
        let mut config = create_minimal_config();
        config.rules = vec![Rule {
            rule_type: "domain_suffix".to_string(),
            value: "example.com".to_string(),
            outbound: "unknown".to_string(),
        }];

        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("points to unknown outbound"));
    }

    #[test]
    fn test_validate_unsupported_rule_type() {
        let mut config = create_minimal_config();
        config.rules = vec![Rule {
            rule_type: "invalid_type".to_string(),
            value: "example.com".to_string(),
            outbound: "direct".to_string(),
        }];

        let result = validate_config(&config);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unsupported rule type"));
    }

    #[test]
    fn test_load_and_save_config() {
        let config = create_minimal_config();

        let mut temp_file = NamedTempFile::new().unwrap();
        let toml_content = toml::to_string_pretty(&config).unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let loaded = load_config(temp_file.path()).unwrap();
        assert_eq!(loaded.general.mode, "socks");
        assert_eq!(loaded.general.listen_port, 1081);
        assert_eq!(loaded.general.final_outbound, "direct");
    }

    #[test]
    fn test_load_invalid_toml() {
        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(b"invalid toml {{{").unwrap();
        temp_file.flush().unwrap();

        let result = load_config(temp_file.path());
        assert!(result.is_err());
    }
}
