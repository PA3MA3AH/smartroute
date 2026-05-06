use crate::config::{load_config, save_config};
use anyhow::{Context, Result};
use std::path::Path;

pub fn merge_nodes(base_path: &Path, nodes_path: &Path, output: Option<&Path>) -> Result<()> {
    tracing::info!(
        base = %base_path.display(),
        nodes = %nodes_path.display(),
        "Starting merge operation"
    );

    let mut base = load_config(base_path)
        .with_context(|| format!("Failed to read base config: {}", base_path.display()))?;

    let fresh = load_config(nodes_path)
        .with_context(|| format!("Failed to read nodes config: {}", nodes_path.display()))?;

    let old_count = base.nodes.len();
    let new_count = fresh.nodes.len();

    base.nodes = fresh.nodes;

    let base_url_empty = base
        .subscription
        .url
        .as_deref()
        .map(str::is_empty)
        .unwrap_or(true);

    let fresh_url_non_empty = fresh
        .subscription
        .url
        .as_deref()
        .map(|url| !url.is_empty())
        .unwrap_or(false);

    if base_url_empty && fresh_url_non_empty {
        tracing::debug!("Setting subscription URL from fresh config");
        base.subscription.url = fresh.subscription.url;
    }

    if base.subscription.auto_refresh == 0 && fresh.subscription.auto_refresh > 0 {
        tracing::debug!(
            auto_refresh = %fresh.subscription.auto_refresh,
            "Setting auto_refresh from fresh config"
        );
        base.subscription.auto_refresh = fresh.subscription.auto_refresh;
    }

    let has_subscription_url = base
        .subscription
        .url
        .as_deref()
        .map(|url| !url.is_empty())
        .unwrap_or(false);

    let output = output.unwrap_or(base_path);

    save_config(output, &base)
        .with_context(|| format!("Failed to save merged config: {}", output.display()))?;

    tracing::info!(
        output = %output.display(),
        old_nodes = %old_count,
        new_nodes = %new_count,
        rules_preserved = %base.rules.len(),
        chains_preserved = %base.chains.len(),
        profiles_preserved = %base.local_profiles.len(),
        has_subscription = %has_subscription_url,
        "Merge completed successfully"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Chain, General, LocalProfile, Node, Rule, SmartRouteConfig, SubscriptionSettings};
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_config(nodes: Vec<Node>, rules: Vec<Rule>, chains: Vec<Chain>, profiles: Vec<LocalProfile>) -> SmartRouteConfig {
        SmartRouteConfig {
            general: General {
                mode: "socks".to_string(),
                listen: "127.0.0.1".to_string(),
                listen_port: 1081,
                final_outbound: "direct".to_string(),
            },
            subscription: SubscriptionSettings::default(),
            nodes,
            chains,
            local_profiles: profiles,
            rules,
        }
    }

    fn create_test_node(tag: &str, port: u16) -> Node {
        Node {
            tag: tag.to_string(),
            node_type: "vless".to_string(),
            server: "example.com".to_string(),
            port,
            uuid: Some("test-uuid".to_string()),
            flow: None,
            security: Some("reality".to_string()),
            server_name: Some("example.com".to_string()),
            utls_fingerprint: Some("chrome".to_string()),
            reality_public_key: Some("test-key".to_string()),
            reality_short_id: Some("test-id".to_string()),
        }
    }

    fn write_config_to_temp(config: &SmartRouteConfig) -> NamedTempFile {
        let mut temp_file = NamedTempFile::new().unwrap();
        let toml_content = toml::to_string_pretty(config).unwrap();
        temp_file.write_all(toml_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();
        temp_file
    }

    #[test]
    fn test_merge_nodes_replaces_nodes() {
        let base_config = create_test_config(
            vec![create_test_node("old-node", 443)],
            vec![],
            vec![],
            vec![],
        );

        let fresh_config = create_test_config(
            vec![
                create_test_node("new-node1", 8443),
                create_test_node("new-node2", 9443),
            ],
            vec![],
            vec![],
            vec![],
        );

        let base_file = write_config_to_temp(&base_config);
        let fresh_file = write_config_to_temp(&fresh_config);
        let output_file = NamedTempFile::new().unwrap();

        merge_nodes(base_file.path(), fresh_file.path(), Some(output_file.path())).unwrap();

        let merged = load_config(output_file.path()).unwrap();
        assert_eq!(merged.nodes.len(), 2);
        assert_eq!(merged.nodes[0].tag, "new-node1");
        assert_eq!(merged.nodes[1].tag, "new-node2");
    }

    #[test]
    fn test_merge_nodes_preserves_rules() {
        let base_config = create_test_config(
            vec![create_test_node("node1", 443)],
            vec![Rule {
                rule_type: "domain_suffix".to_string(),
                value: "example.com".to_string(),
                outbound: "node1".to_string(),
            }],
            vec![],
            vec![],
        );

        let fresh_config = create_test_config(
            vec![create_test_node("node2", 8443)],
            vec![],
            vec![],
            vec![],
        );

        let base_file = write_config_to_temp(&base_config);
        let fresh_file = write_config_to_temp(&fresh_config);
        let output_file = NamedTempFile::new().unwrap();

        merge_nodes(base_file.path(), fresh_file.path(), Some(output_file.path())).unwrap();

        let merged = load_config(output_file.path()).unwrap();
        assert_eq!(merged.rules.len(), 1);
        assert_eq!(merged.rules[0].value, "example.com");
        assert_eq!(merged.rules[0].outbound, "node1");
    }

    #[test]
    fn test_merge_nodes_preserves_chains() {
        let base_config = create_test_config(
            vec![
                create_test_node("node1", 443),
                create_test_node("node2", 8443),
            ],
            vec![],
            vec![Chain {
                tag: "chain1".to_string(),
                outbounds: vec!["node1".to_string(), "node2".to_string()],
            }],
            vec![],
        );

        let fresh_config = create_test_config(
            vec![create_test_node("node3", 9443)],
            vec![],
            vec![],
            vec![],
        );

        let base_file = write_config_to_temp(&base_config);
        let fresh_file = write_config_to_temp(&fresh_config);
        let output_file = NamedTempFile::new().unwrap();

        merge_nodes(base_file.path(), fresh_file.path(), Some(output_file.path())).unwrap();

        let merged = load_config(output_file.path()).unwrap();
        assert_eq!(merged.chains.len(), 1);
        assert_eq!(merged.chains[0].tag, "chain1");
        assert_eq!(merged.chains[0].outbounds.len(), 2);
    }

    #[test]
    fn test_merge_nodes_preserves_local_profiles() {
        let base_config = create_test_config(
            vec![create_test_node("node1", 443)],
            vec![],
            vec![],
            vec![LocalProfile {
                tag: "profile1".to_string(),
                listen: "127.0.0.1".to_string(),
                listen_port: 1082,
                outbound: "node1".to_string(),
            }],
        );

        let fresh_config = create_test_config(
            vec![create_test_node("node2", 8443)],
            vec![],
            vec![],
            vec![],
        );

        let base_file = write_config_to_temp(&base_config);
        let fresh_file = write_config_to_temp(&fresh_config);
        let output_file = NamedTempFile::new().unwrap();

        merge_nodes(base_file.path(), fresh_file.path(), Some(output_file.path())).unwrap();

        let merged = load_config(output_file.path()).unwrap();
        assert_eq!(merged.local_profiles.len(), 1);
        assert_eq!(merged.local_profiles[0].tag, "profile1");
        assert_eq!(merged.local_profiles[0].listen_port, 1082);
    }

    #[test]
    fn test_merge_nodes_preserves_general_settings() {
        let mut base_config = create_test_config(vec![], vec![], vec![], vec![]);
        base_config.general.mode = "tun".to_string();
        base_config.general.listen_port = 2080;
        base_config.general.final_outbound = "custom".to_string();

        let fresh_config = create_test_config(
            vec![create_test_node("node1", 443)],
            vec![],
            vec![],
            vec![],
        );

        let base_file = write_config_to_temp(&base_config);
        let fresh_file = write_config_to_temp(&fresh_config);
        let output_file = NamedTempFile::new().unwrap();

        merge_nodes(base_file.path(), fresh_file.path(), Some(output_file.path())).unwrap();

        let merged = load_config(output_file.path()).unwrap();
        assert_eq!(merged.general.mode, "tun");
        assert_eq!(merged.general.listen_port, 2080);
        assert_eq!(merged.general.final_outbound, "custom");
    }

    #[test]
    fn test_merge_nodes_updates_subscription_url() {
        let base_config = create_test_config(vec![], vec![], vec![], vec![]);

        let mut fresh_config = create_test_config(vec![create_test_node("node1", 443)], vec![], vec![], vec![]);
        fresh_config.subscription.url = Some("https://example.com/sub".to_string());

        let base_file = write_config_to_temp(&base_config);
        let fresh_file = write_config_to_temp(&fresh_config);
        let output_file = NamedTempFile::new().unwrap();

        merge_nodes(base_file.path(), fresh_file.path(), Some(output_file.path())).unwrap();

        let merged = load_config(output_file.path()).unwrap();
        assert_eq!(merged.subscription.url, Some("https://example.com/sub".to_string()));
    }

    #[test]
    fn test_merge_nodes_keeps_existing_subscription_url() {
        let mut base_config = create_test_config(vec![], vec![], vec![], vec![]);
        base_config.subscription.url = Some("https://old.com/sub".to_string());

        let mut fresh_config = create_test_config(vec![create_test_node("node1", 443)], vec![], vec![], vec![]);
        fresh_config.subscription.url = Some("https://new.com/sub".to_string());

        let base_file = write_config_to_temp(&base_config);
        let fresh_file = write_config_to_temp(&fresh_config);
        let output_file = NamedTempFile::new().unwrap();

        merge_nodes(base_file.path(), fresh_file.path(), Some(output_file.path())).unwrap();

        let merged = load_config(output_file.path()).unwrap();
        assert_eq!(merged.subscription.url, Some("https://old.com/sub".to_string()));
    }

    #[test]
    fn test_merge_nodes_default_output_to_base() {
        let base_config = create_test_config(
            vec![create_test_node("old-node", 443)],
            vec![],
            vec![],
            vec![],
        );

        let fresh_config = create_test_config(
            vec![create_test_node("new-node", 8443)],
            vec![],
            vec![],
            vec![],
        );

        let base_file = write_config_to_temp(&base_config);
        let fresh_file = write_config_to_temp(&fresh_config);

        merge_nodes(base_file.path(), fresh_file.path(), None).unwrap();

        let merged = load_config(base_file.path()).unwrap();
        assert_eq!(merged.nodes.len(), 1);
        assert_eq!(merged.nodes[0].tag, "new-node");
    }
}
