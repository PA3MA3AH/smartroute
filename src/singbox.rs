use crate::config::SmartRouteConfig;
use anyhow::Result;
use serde_json::{json, Value};

pub fn generate_singbox_config(config: &SmartRouteConfig) -> Result<Value> {
    let inbound = match config.general.mode.as_str() {
        "socks" => json!({
            "type": "socks",
            "tag": "socks-in",
            "listen": config.general.listen,
            "listen_port": config.general.listen_port
        }),
        "tun" => json!({
            "type": "tun",
            "tag": "tun-in",
            "interface_name": "smartroute0",
            "address": [
                "172.19.0.1/30"
            ],
            "auto_route": true,
            "strict_route": false
        }),
        other => anyhow::bail!("Unsupported mode: {}", other),
    };

    let mut outbounds = Vec::new();

    outbounds.push(json!({
        "type": "direct",
        "tag": "direct"
    }));

    outbounds.push(json!({
        "type": "block",
        "tag": "block"
    }));

    for node in &config.nodes {
        match node.node_type.as_str() {
            "socks" => {
                outbounds.push(json!({
                    "type": "socks",
                    "tag": node.tag,
                    "server": node.server,
                    "server_port": node.port
                }));
            }

            "vless" => {
                let mut outbound = json!({
                    "type": "vless",
                    "tag": node.tag,
                    "server": node.server,
                    "server_port": node.port,
                    "uuid": node.uuid.as_deref().unwrap_or("")
                });

                if let Some(flow) = &node.flow {
                    outbound["flow"] = json!(flow);
                }

                match node.security.as_deref() {
                    Some("tls") => {
                        outbound["tls"] = json!({
                            "enabled": true,
                            "server_name": node.server_name.as_deref().unwrap_or(&node.server),
                            "utls": {
                                "enabled": true,
                                "fingerprint": "chrome"
                            }
                        });
                    }

                    Some("reality") => {
                        outbound["tls"] = json!({
                            "enabled": true,
                            "server_name": node.server_name.as_deref().unwrap_or(&node.server),
                            "utls": {
                                "enabled": true,
                                "fingerprint": "chrome"
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

                outbounds.push(outbound);
            }

            other => {
                anyhow::bail!("Unsupported node type: {}", other);
            }
        }
    }

    let mut rules = Vec::new();

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
        "inbounds": [
            inbound
        ],
        "outbounds": outbounds,
        "route": {
            "rules": rules,
            "final": config.general.final_outbound
        }
    }))
}
