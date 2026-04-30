use crate::config::{Node, SmartRouteConfig};
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::HashMap;

pub fn generate_singbox_config(config: &SmartRouteConfig) -> Result<Value> {
    let mut inbounds = Vec::new();

    match config.general.mode.as_str() {
        "socks" => {
            inbounds.push(json!({
                "type": "socks",
                "tag": "socks-in",
                "listen": config.general.listen,
                "listen_port": config.general.listen_port
            }));
        }

        "tun" => {
            inbounds.push(json!({
                "type": "tun",
                "tag": "tun-in",
                "interface_name": "smartroute0",
                "address": [
                    "172.19.0.1/30"
                ],
                "auto_route": true,
                "strict_route": false
            }));
        }

        other => {
            anyhow::bail!("Unsupported mode: {}", other);
        }
    }

    for profile in &config.local_profiles {
        inbounds.push(json!({
            "type": "socks",
            "tag": profile.tag,
            "listen": profile.listen,
            "listen_port": profile.listen_port
        }));
    }

    let mut outbounds = Vec::new();

    outbounds.push(json!({
        "type": "direct",
        "tag": "direct"
    }));

    outbounds.push(json!({
        "type": "block",
        "tag": "block"
    }));

    let node_map: HashMap<String, Node> = config
        .nodes
        .iter()
        .map(|node| (node.tag.clone(), node.clone()))
        .collect();

    for node in &config.nodes {
        outbounds.push(node_to_outbound(node, &node.tag, None)?);
    }

    for chain in &config.chains {
        if chain.outbounds.len() < 2 {
            anyhow::bail!("Chain {} must contain at least 2 outbounds", chain.tag);
        }

        let mut previous = chain.outbounds[0].clone();

        for (idx, tag) in chain.outbounds.iter().enumerate().skip(1) {
            let node = node_map.get(tag).ok_or_else(|| {
                anyhow::anyhow!(
                    "Chain {} references unsupported/unknown outbound: {}",
                    chain.tag,
                    tag
                )
            })?;

            let generated_tag = if idx + 1 == chain.outbounds.len() {
                chain.tag.clone()
            } else {
                format!("{}__hop{}", chain.tag, idx + 1)
            };

            outbounds.push(node_to_outbound(node, &generated_tag, Some(&previous))?);
            previous = generated_tag;
        }
    }

    let mut rules = Vec::new();

    for profile in &config.local_profiles {
        rules.push(json!({
            "inbound": [profile.tag],
            "outbound": profile.outbound
        }));
    }

    for rule in &config.rules {
        match rule.rule_type.as_str() {
            "domain" => {
                rules.push(json!({
                    "domain": [rule.value],
                    "outbound": rule.outbound
                }));
            }

            "domain_suffix" => {
                rules.push(json!({
                    "domain_suffix": [rule.value],
                    "outbound": rule.outbound
                }));
            }

            "domain_keyword" => {
                rules.push(json!({
                    "domain_keyword": [rule.value],
                    "outbound": rule.outbound
                }));
            }

            other => {
                anyhow::bail!("Unsupported rule type: {}", other);
            }
        }
    }

    Ok(json!({
        "log": {
            "level": "info"
        },
        "inbounds": inbounds,
        "outbounds": outbounds,
        "route": {
            "rules": rules,
            "final": config.general.final_outbound
        }
    }))
}

fn node_to_outbound(node: &Node, tag: &str, detour: Option<&str>) -> Result<Value> {
    let mut outbound = match node.node_type.as_str() {
        "socks" => {
            json!({
                "type": "socks",
                "tag": tag,
                "server": node.server,
                "server_port": node.port
            })
        }

        "vless" => {
            let mut outbound = json!({
                "type": "vless",
                "tag": tag,
                "server": node.server,
                "server_port": node.port,
                "uuid": node.uuid.as_deref().unwrap_or("")
            });

            if let Some(flow) = &node.flow {
                outbound["flow"] = json!(flow);
            }

            let fingerprint = node.utls_fingerprint.as_deref().unwrap_or("chrome");

            match node.security.as_deref() {
                Some("tls") => {
                    outbound["tls"] = json!({
                        "enabled": true,
                        "server_name": node.server_name.as_deref().unwrap_or(&node.server),
                        "utls": {
                            "enabled": true,
                            "fingerprint": fingerprint
                        }
                    });
                }

                Some("reality") => {
                    outbound["tls"] = json!({
                        "enabled": true,
                        "server_name": node.server_name.as_deref().unwrap_or(&node.server),
                        "utls": {
                            "enabled": true,
                            "fingerprint": fingerprint
                        },
                        "reality": {
                            "enabled": true,
                            "public_key": node.reality_public_key.as_deref().unwrap_or(""),
                            "short_id": node.reality_short_id.as_deref().unwrap_or("")
                        }
                    });
                }

                _ => {}
            }

            outbound
        }

        other => {
            anyhow::bail!("Unsupported node type: {}", other);
        }
    };

    if let Some(upstream) = detour {
        outbound["detour"] = json!(upstream);
    }

    Ok(outbound)
}
