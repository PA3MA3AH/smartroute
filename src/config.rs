use anyhow::{Context, Result};
use serde::Deserialize;
use std::{collections::HashSet, fs, path::Path};

#[derive(Debug, Deserialize, Clone)]
pub struct SmartRouteConfig {
    pub general: General,

    #[serde(default)]
    pub nodes: Vec<Node>,

    #[serde(default)]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct General {
    #[serde(default = "default_mode")]
    pub mode: String,

    #[serde(default = "default_listen")]
    pub listen: String,

    #[serde(default = "default_listen_port")]
    pub listen_port: u16,

    pub final_outbound: String,
}

#[derive(Debug, Deserialize, Clone)]
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
    pub reality_public_key: Option<String>,

    #[serde(default)]
    pub reality_short_id: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
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

    outbounds.insert("direct".to_string());
    outbounds.insert("block".to_string());

    match config.general.mode.as_str() {
        "socks" | "tun" => {}
        other => anyhow::bail!("Unsupported mode: {}. Supported: socks, tun", other),
    }

    if config.general.listen.trim().is_empty() {
        anyhow::bail!("general.listen cannot be empty");
    }

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

    if !outbounds.contains(&config.general.final_outbound) {
        anyhow::bail!(
            "final_outbound points to unknown outbound: {}",
            config.general.final_outbound
        );
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
