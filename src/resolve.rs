use crate::config::{load_config, save_config};
use anyhow::{Context, Result};
use std::{
    net::{IpAddr, ToSocketAddrs},
    path::Path,
};

pub fn resolve_domains_to_ip(config_path: &Path) -> Result<usize> {
    let mut config = load_config(config_path)?;

    let mut changed = 0usize;

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

        println!("Resolved {} -> {}", server, ip);

        node.server = ip.to_string();
        changed += 1;
    }

    if changed > 0 {
        save_config(config_path, &config)
            .with_context(|| format!("Failed to save resolved config {}", config_path.display()))?;
    }

    Ok(changed)
}
