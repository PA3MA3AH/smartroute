use crate::{config::load_config, resolve::resolve_domains_to_ip};
use anyhow::{Context, Result};
use std::{net::IpAddr, path::Path, process::Command};

const FIREWALL_GROUP: &str = "SmartRoute KillSwitch";

pub fn enable_killswitch(config_path: &Path, smart_mode: bool) -> Result<()> {
    enable_killswitch_impl(config_path, smart_mode)
}

pub fn disable_killswitch() -> Result<()> {
    disable_killswitch_impl()
}

pub fn status_killswitch() -> Result<()> {
    status_killswitch_impl()
}

#[cfg(not(windows))]
fn enable_killswitch_impl(config_path: &Path, smart_mode: bool) -> Result<()> {
    let resolved = resolve_domains_to_ip(config_path)?;
    if resolved > 0 {
        println!("Resolved {} proxy domain(s) before kill-switch", resolved);
    }

    let config = load_config(config_path)?;

    let mut proxy4 = Vec::new();
    let mut proxy6 = Vec::new();

    for node in &config.nodes {
        let server = node.server.trim();
        let port = node.port;

        let ip = server
            .parse::<IpAddr>()
            .with_context(|| format!("Proxy node is still not IP after resolve: {}", server))?;

        match ip {
            IpAddr::V4(v4) => proxy4.push(format!("{} . {}", v4, port)),
            IpAddr::V6(v6) => proxy6.push(format!("{} . {}", v6, port)),
        }
    }

    proxy4.sort();
    proxy4.dedup();
    proxy6.sort();
    proxy6.dedup();

    if proxy4.is_empty() && proxy6.is_empty() {
        anyhow::bail!("No proxy nodes found in config. Kill-switch cannot be enabled.");
    }

    let _ = Command::new("nft")
        .args(["delete", "table", "inet", "smartroute"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    let mut rules = String::new();

    rules.push_str("table inet smartroute {\n");

    if !proxy4.is_empty() {
        rules.push_str("  set proxy4 {\n");
        rules.push_str("    type ipv4_addr . inet_service\n");
        rules.push_str("    elements = { ");
        rules.push_str(&proxy4.join(", "));
        rules.push_str(" }\n");
        rules.push_str("  }\n");
    }

    if !proxy6.is_empty() {
        rules.push_str("  set proxy6 {\n");
        rules.push_str("    type ipv6_addr . inet_service\n");
        rules.push_str("    elements = { ");
        rules.push_str(&proxy6.join(", "));
        rules.push_str(" }\n");
        rules.push_str("  }\n");
    }

    rules.push_str("  chain output {\n");
    rules.push_str("    type filter hook output priority 0; policy drop;\n");
    rules.push_str("    oifname \"lo\" accept\n");
    rules.push_str("    ct state established,related accept\n");

    rules.push_str("    ip daddr 10.0.0.0/8 accept\n");
    rules.push_str("    ip daddr 172.16.0.0/12 accept\n");
    rules.push_str("    ip daddr 192.168.0.0/16 accept\n");
    rules.push_str("    ip daddr 127.0.0.0/8 accept\n");
    rules.push_str("    ip6 daddr ::1 accept\n");
    rules.push_str("    ip6 daddr fe80::/10 accept\n");

    if !proxy4.is_empty() {
        rules.push_str("    ip daddr . tcp dport @proxy4 accept\n");
        rules.push_str("    ip daddr . udp dport @proxy4 accept\n");
    }

    if !proxy6.is_empty() {
        rules.push_str("    ip6 daddr . tcp dport @proxy6 accept\n");
        rules.push_str("    ip6 daddr . udp dport @proxy6 accept\n");
    }

    rules.push_str("  }\n");
    rules.push_str("}\n");

    apply_nft(&rules)?;

    print_enabled_message(smart_mode);
    Ok(())
}

#[cfg(windows)]
fn enable_killswitch_impl(config_path: &Path, smart_mode: bool) -> Result<()> {
    let resolved = resolve_domains_to_ip(config_path)?;
    if resolved > 0 {
        println!("Resolved {} proxy domain(s) before kill-switch", resolved);
    }

    let config = load_config(config_path)?;
    let mut proxy_ips = Vec::new();

    for node in &config.nodes {
        let server = node.server.trim();
        let ip = server
            .parse::<IpAddr>()
            .with_context(|| format!("Proxy node is still not IP after resolve: {}", server))?;
        proxy_ips.push(ip.to_string());
    }

    proxy_ips.sort();
    proxy_ips.dedup();

    if proxy_ips.is_empty() {
        anyhow::bail!("No proxy nodes found in config. Kill-switch cannot be enabled.");
    }

    let _ = disable_killswitch_impl();

    for ip in proxy_ips {
        run_netsh(&[
            "advfirewall",
            "firewall",
            "add",
            "rule",
            &format!("name=SmartRoute Allow Proxy {}", ip),
            &format!("group={}", FIREWALL_GROUP),
            "dir=out",
            "action=allow",
            "enable=yes",
            "profile=any",
            "protocol=any",
            &format!("remoteip={}", ip),
        ])?;
    }

    run_netsh(&[
        "advfirewall",
        "firewall",
        "add",
        "rule",
        "name=SmartRoute Allow Loopback",
        &format!("group={}", FIREWALL_GROUP),
        "dir=out",
        "action=allow",
        "enable=yes",
        "profile=any",
        "protocol=any",
        "remoteip=127.0.0.1,::1",
    ])?;

    run_netsh(&[
        "advfirewall",
        "firewall",
        "add",
        "rule",
        "name=SmartRoute Allow LAN",
        &format!("group={}", FIREWALL_GROUP),
        "dir=out",
        "action=allow",
        "enable=yes",
        "profile=any",
        "protocol=any",
        "remoteip=10.0.0.0/8,172.16.0.0/12,192.168.0.0/16,169.254.0.0/16,fe80::/10",
    ])?;

    run_netsh(&[
        "advfirewall",
        "firewall",
        "add",
        "rule",
        "name=SmartRoute Block Outbound",
        &format!("group={}", FIREWALL_GROUP),
        "dir=out",
        "action=block",
        "enable=yes",
        "profile=any",
        "protocol=any",
    ])?;

    print_enabled_message(smart_mode);
    println!("Windows Firewall group: {}", FIREWALL_GROUP);
    println!("Note: run this command from an elevated Administrator terminal.");
    Ok(())
}

#[cfg(not(windows))]
fn disable_killswitch_impl() -> Result<()> {
    let status = Command::new("nft")
        .args(["delete", "table", "inet", "smartroute"])
        .status()
        .context("Failed to run nft")?;

    if status.success() {
        println!("Kill-switch disabled");
    } else {
        println!("Kill-switch was probably already disabled");
    }

    Ok(())
}

#[cfg(windows)]
fn disable_killswitch_impl() -> Result<()> {
    let status = Command::new("netsh")
        .args([
            "advfirewall",
            "firewall",
            "delete",
            "rule",
            &format!("group={}", FIREWALL_GROUP),
        ])
        .status()
        .context("Failed to run netsh")?;

    if status.success() {
        println!("Kill-switch disabled");
    } else {
        println!("Kill-switch was probably already disabled");
    }

    Ok(())
}

#[cfg(not(windows))]
fn status_killswitch_impl() -> Result<()> {
    let status = Command::new("nft")
        .args(["list", "table", "inet", "smartroute"])
        .status()
        .context("Failed to run nft")?;

    if status.success() {
        println!("Kill-switch: enabled");
    } else {
        println!("Kill-switch: disabled");
    }

    Ok(())
}

#[cfg(windows)]
fn status_killswitch_impl() -> Result<()> {
    let status = Command::new("netsh")
        .args([
            "advfirewall",
            "firewall",
            "show",
            "rule",
            &format!("group={}", FIREWALL_GROUP),
        ])
        .status()
        .context("Failed to run netsh")?;

    if status.success() {
        println!("Kill-switch: enabled");
    } else {
        println!("Kill-switch: disabled");
    }

    Ok(())
}

#[cfg(not(windows))]
fn apply_nft(rules: &str) -> Result<()> {
    let mut child = Command::new("nft")
        .arg("-f")
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .context("Failed to start nft. Is nftables installed?")?;

    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().context("Failed to open nft stdin")?;
        stdin.write_all(rules.as_bytes())?;
    }

    let status = child.wait()?;

    if !status.success() {
        anyhow::bail!("nft failed to apply kill-switch rules");
    }

    Ok(())
}

#[cfg(windows)]
fn run_netsh(args: &[&str]) -> Result<()> {
    let status = Command::new("netsh")
        .args(args)
        .status()
        .context("Failed to run netsh")?;

    if !status.success() {
        anyhow::bail!("netsh command failed: netsh {}", args.join(" "));
    }

    Ok(())
}

fn print_enabled_message(smart_mode: bool) {
    println!("Kill-switch enabled");
    if smart_mode {
        println!("Mode: proxy-only");
        println!("Direct outbound is blocked. All traffic should go through proxy/chain.");
    } else {
        println!("Mode: strict");
    }
}
