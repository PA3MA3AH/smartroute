use crate::config::{Rule, load_config, save_config};
use anyhow::{Context, Result};
use std::{
    net::{IpAddr, ToSocketAddrs},
    path::Path,
};

pub fn resolve_domains_to_ip(config_path: &Path) -> Result<usize> {
    let mut config = load_config(config_path)?;
    let mut changed = 0usize;

    // Resolve proxy node hostnames to IPs
    for node in &mut config.nodes {
        let server = node.server.trim().to_string();

        if server.parse::<IpAddr>().is_ok() {
            continue;
        }

        let addr = format!("{}:{}", server, node.port);

        let mut addrs = match addr.to_socket_addrs() {
            Ok(addrs) => addrs.collect::<Vec<_>>(),
            Err(err) => {
                eprintln!(
                    "Warning: failed to resolve proxy node {}:{}: {}",
                    server, node.port, err
                );
                continue;
            }
        };

        if addrs.is_empty() {
            eprintln!("Warning: no DNS records for {}", server);
            continue;
        }

        addrs.sort_by_key(|addr| match addr.ip() {
            IpAddr::V4(_) => 0,
            IpAddr::V6(_) => 1,
        });

        let ip = addrs[0].ip();
        println!("Resolved node {} -> {}", server, ip);
        node.server = ip.to_string();
        changed += 1;
    }

    // Resolve domain/domain_suffix rules to ip_cidr rules.
    // For each domain rule that has no corresponding ip_cidr rule yet,
    // resolve the domain and append an ip_cidr rule with the same outbound.
    let existing_ip_cidrs: Vec<(String, String)> = config
        .rules
        .iter()
        .filter(|r| r.rule_type == "ip_cidr")
        .map(|r| (r.value.clone(), r.outbound.clone()))
        .collect();

    let mut new_ip_rules: Vec<Rule> = Vec::new();

    for rule in &config.rules {
        if rule.rule_type != "domain" && rule.rule_type != "domain_suffix" {
            continue;
        }

        let domain = rule.value.trim_start_matches('.');

        // Resolve to IPs
        let addr = format!("{}:443", domain);
        let addrs = match addr.to_socket_addrs() {
            Ok(a) => a.collect::<Vec<_>>(),
            Err(_) => continue,
        };

        for socket_addr in addrs {
            let ip = socket_addr.ip();
            let cidr = match ip {
                IpAddr::V4(v4) => format!("{}/32", v4),
                IpAddr::V6(v6) => format!("{}/128", v6),
            };

            // Skip if already present for this outbound
            if existing_ip_cidrs
                .iter()
                .any(|(v, o)| v == &cidr && o == &rule.outbound)
            {
                continue;
            }

            // Skip if already in new_ip_rules
            if new_ip_rules
                .iter()
                .any(|r| r.value == cidr && r.outbound == rule.outbound)
            {
                continue;
            }

            println!(
                "Resolved rule domain {} -> ip_cidr {} ({})",
                domain, cidr, rule.outbound
            );

            new_ip_rules.push(Rule {
                rule_type: "ip_cidr".to_string(),
                value: cidr,
                outbound: rule.outbound.clone(),
            });

            changed += 1;
        }
    }

    config.rules.extend(new_ip_rules);

    if changed > 0 {
        save_config(config_path, &config)
            .with_context(|| format!("Failed to save resolved config {}", config_path.display()))?;
    }

    Ok(changed)
}
