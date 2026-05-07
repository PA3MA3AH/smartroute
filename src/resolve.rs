use crate::config::{load_config, save_config};
use anyhow::{Context, Result};
use std::{
    net::{IpAddr, ToSocketAddrs},
    path::Path,
};

/// Fake-ip range used by sing-box TUN mode (198.18.0.0/15).
/// IPs in this range must never be saved to config.
fn is_fake_ip(ip: IpAddr) -> bool {
    if let IpAddr::V4(v4) = ip {
        let o = v4.octets();
        // 198.18.0.0/15 = 198.18.x.x and 198.19.x.x
        o[0] == 198 && (o[1] == 18 || o[1] == 19)
    } else {
        false
    }
}

pub fn resolve_domains_to_ip(config_path: &Path) -> Result<usize> {
    let mut config = load_config(config_path)?;
    let mut changed = 0usize;

    // Remove any ip_cidr rules that contain fake-ip addresses (198.18.0.0/15).
    // These were incorrectly added by a previous version of this function.
    let before = config.rules.len();
    config.rules.retain(|r| {
        if r.rule_type != "ip_cidr" {
            return true;
        }
        let ip_str = r.value.split('/').next().unwrap_or("");
        match ip_str.parse::<IpAddr>() {
            Ok(ip) if is_fake_ip(ip) => false,
            _ => true,
        }
    });
    let removed_fake = before - config.rules.len();
    if removed_fake > 0 {
        eprintln!("Removed {} fake-ip ip_cidr rules (198.18.x.x)", removed_fake);
        changed += removed_fake;
    }

    // Resolve proxy node hostnames to IPs.
    // Only nodes whose server field is a domain name (not already an IP).
    for node in &mut config.nodes {
        let server = node.server.trim().to_string();

        if server.parse::<IpAddr>().is_ok() {
            continue;
        }

        let addr = format!("{}:{}", server, node.port);

        let mut addrs = match addr.to_socket_addrs() {
            Ok(a) => a.collect::<Vec<_>>(),
            Err(err) => {
                eprintln!("Warning: failed to resolve node {}:{}: {}", server, node.port, err);
                continue;
            }
        };

        if addrs.is_empty() {
            eprintln!("Warning: no DNS records for {}", server);
            continue;
        }

        addrs.sort_by_key(|a| match a.ip() {
            IpAddr::V4(_) => 0,
            IpAddr::V6(_) => 1,
        });

        let ip = addrs[0].ip();

        if is_fake_ip(ip) {
            eprintln!(
                "Warning: resolved {} to fake-ip {} — sing-box TUN may be active, skipping",
                server, ip
            );
            continue;
        }

        println!("Resolved node {} -> {}", server, ip);
        node.server = ip.to_string();
        changed += 1;
    }

    if changed > 0 {
        save_config(config_path, &config)
            .with_context(|| format!("Failed to save resolved config {}", config_path.display()))?;
    }

    Ok(changed)
}
