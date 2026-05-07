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
                "address": ["172.19.0.1/30"],
                "auto_route": true,
                "strict_route": false
            }));
        }

        other => anyhow::bail!("Unsupported mode: {}", other),
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

    outbounds.push(json!({ "type": "direct", "tag": "direct" }));
    outbounds.push(json!({ "type": "block", "tag": "block" }));

    let node_map: HashMap<String, Node> = config
        .nodes
        .iter()
        .map(|n| (n.tag.clone(), n.clone()))
        .collect();

    for node in &config.nodes {
        outbounds.push(node_to_outbound(node, &node.tag, None)?);
    }

    // Chain proxy: app -> proxy-a -> proxy-b -> internet
    // In sing-box: proxy-b has detour = proxy-a (proxy-b connects through proxy-a)
    // So we iterate outbounds[0..n-1] as the "upstream" chain,
    // and each next hop gets detour pointing to the previous one.
    //
    // Example: outbounds = ["proxy-a", "proxy-b"]
    //   proxy-a: no detour (connects directly)
    //   proxy-b (tagged as chain.tag): detour = "proxy-a"
    //
    // For longer chains: ["a", "b", "c"]
    //   a: no detour
    //   b (tagged chain__hop1): detour = "a"
    //   c (tagged chain.tag): detour = chain__hop1
    for chain in &config.chains {
        if chain.outbounds.len() < 2 {
            anyhow::bail!("Chain {} must contain at least 2 outbounds", chain.tag);
        }

        // First hop: already emitted as a standalone node above (no detour needed).
        // We only need to emit hops 1..n with detour pointing to the previous tag.
        let mut previous_tag = chain.outbounds[0].clone();

        for (idx, outbound_tag) in chain.outbounds.iter().enumerate().skip(1) {
            let node = node_map.get(outbound_tag).ok_or_else(|| {
                anyhow::anyhow!(
                    "Chain {} references unknown outbound: {}",
                    chain.tag,
                    outbound_tag
                )
            })?;

            // Last hop gets the chain's own tag so rules can reference it directly.
            let generated_tag = if idx + 1 == chain.outbounds.len() {
                chain.tag.clone()
            } else {
                format!("{}__hop{}", chain.tag, idx)
            };

            outbounds.push(node_to_outbound(node, &generated_tag, Some(&previous_tag))?);
            previous_tag = generated_tag;
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
        let entry = match rule.rule_type.as_str() {
            "domain" => json!({ "domain": [rule.value], "outbound": rule.outbound }),
            "domain_suffix" => json!({ "domain_suffix": [rule.value], "outbound": rule.outbound }),
            "domain_keyword" => json!({ "domain_keyword": [rule.value], "outbound": rule.outbound }),
            "ip_cidr" => json!({ "ip_cidr": [rule.value], "outbound": rule.outbound }),
            "geoip" => json!({ "geoip": [rule.value], "outbound": rule.outbound }),
            "geosite" => json!({ "geosite": [rule.value], "outbound": rule.outbound }),
            other => anyhow::bail!("Unsupported rule type: {}", other),
        };
        rules.push(entry);
    }

    Ok(json!({
        "log": { "level": "info" },
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
        "socks" => json!({
            "type": "socks",
            "tag": tag,
            "server": node.server,
            "server_port": node.port
        }),

        "vless" => {
            let mut ob = json!({
                "type": "vless",
                "tag": tag,
                "server": node.server,
                "server_port": node.port,
                "uuid": node.uuid.as_deref().unwrap_or("")
            });

            if let Some(flow) = &node.flow {
                ob["flow"] = json!(flow);
            }

            let fingerprint = node.utls_fingerprint.as_deref().unwrap_or("chrome");

            match node.security.as_deref() {
                Some("tls") => {
                    ob["tls"] = json!({
                        "enabled": true,
                        "server_name": node.server_name.as_deref().unwrap_or(&node.server),
                        "utls": { "enabled": true, "fingerprint": fingerprint }
                    });
                }
                Some("reality") => {
                    ob["tls"] = json!({
                        "enabled": true,
                        "server_name": node.server_name.as_deref().unwrap_or(&node.server),
                        "utls": { "enabled": true, "fingerprint": fingerprint },
                        "reality": {
                            "enabled": true,
                            "public_key": node.reality_public_key.as_deref().unwrap_or(""),
                            "short_id": node.reality_short_id.as_deref().unwrap_or("")
                        }
                    });
                }
                _ => {}
            }

            ob
        }

        "vmess" => {
            let mut ob = json!({
                "type": "vmess",
                "tag": tag,
                "server": node.server,
                "server_port": node.port,
                "uuid": node.uuid.as_deref().unwrap_or(""),
                "alter_id": node.alter_id.unwrap_or(0),
                "security": "auto"
            });

            let fingerprint = node.utls_fingerprint.as_deref().unwrap_or("chrome");

            match node.security.as_deref() {
                Some("tls") => {
                    ob["tls"] = json!({
                        "enabled": true,
                        "server_name": node.server_name.as_deref().unwrap_or(&node.server),
                        "utls": { "enabled": true, "fingerprint": fingerprint }
                    });
                }
                _ => {}
            }

            ob
        }

        "shadowsocks" => {
            json!({
                "type": "shadowsocks",
                "tag": tag,
                "server": node.server,
                "server_port": node.port,
                "method": node.method.as_deref().unwrap_or("aes-128-gcm"),
                "password": node.password.as_deref().unwrap_or("")
            })
        }

        "trojan" => {
            let mut ob = json!({
                "type": "trojan",
                "tag": tag,
                "server": node.server,
                "server_port": node.port,
                "password": node.password.as_deref().unwrap_or("")
            });

            let fingerprint = node.utls_fingerprint.as_deref().unwrap_or("chrome");

            ob["tls"] = json!({
                "enabled": true,
                "server_name": node.server_name.as_deref().unwrap_or(&node.server),
                "utls": { "enabled": true, "fingerprint": fingerprint }
            });

            ob
        }

        "hysteria2" => {
            let mut ob = json!({
                "type": "hysteria2",
                "tag": tag,
                "server": node.server,
                "server_port": node.port,
                "password": node.password.as_deref().unwrap_or("")
            });

            if let Some(up) = node.up_mbps {
                ob["up_mbps"] = json!(up);
            }
            if let Some(down) = node.down_mbps {
                ob["down_mbps"] = json!(down);
            }

            if let (Some(obfs_type), Some(obfs_pwd)) = (&node.obfs, &node.obfs_password) {
                ob["obfs"] = json!({
                    "type": obfs_type,
                    "password": obfs_pwd
                });
            }

            ob["tls"] = json!({
                "enabled": true,
                "server_name": node.server_name.as_deref().unwrap_or(&node.server)
            });

            ob
        }

        other => anyhow::bail!("Unsupported node type: {}", other),
    };

    if let Some(upstream) = detour {
        outbound["detour"] = json!(upstream);
    }

    Ok(outbound)
}
