use crate::config::load_config;
use anyhow::{Context, Result};
use std::{
    collections::{BTreeSet, HashSet},
    net::IpAddr,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

pub fn run_leak_test(input: &Path, domain: &str, interface: Option<&str>) -> Result<()> {
    let config = load_config(input)?;
    let host = normalize_host(domain);
    let url = format!("https://{}", host);

    println!("SmartRoute leak-test");
    println!("────────────────────────────────────────────────────────");
    println!("Config: {}", input.display());
    println!("Test domain: {}", host);
    println!(
        "SOCKS5: {}:{}",
        config.general.listen, config.general.listen_port
    );
    println!();

    let mut failed = 0usize;

    if check_killswitch_enabled() {
        ok("kill-switch is enabled");
    } else {
        fail("kill-switch is disabled");
        failed += 1;
    }

    if config.general.final_outbound == "direct" {
        fail("final_outbound is direct");
        failed += 1;
    } else {
        ok(&format!(
            "final_outbound is proxy/chain: {}",
            config.general.final_outbound
        ));
    }

    let direct_rules = config
        .rules
        .iter()
        .filter(|rule| rule.outbound == "direct")
        .count();

    if direct_rules == 0 {
        ok("no rules use outbound = direct");
    } else {
        fail(&format!("{} rule(s) use outbound = direct", direct_rules));
        failed += 1;
    }

    println!();
    println!("Direct connection test:");
    let direct_ok = curl_head(&url, None, 5)?;

    if direct_ok {
        fail("direct curl succeeded, this is a leak");
        failed += 1;
    } else {
        ok("direct curl is blocked");
    }

    println!();
    println!("SOCKS connection test:");
    let socks = format!("{}:{}", config.general.listen, config.general.listen_port);
    let socks_ok = curl_head(&url, Some(&socks), 15)?;

    if socks_ok {
        ok("SOCKS curl works through SmartRoute");
    } else {
        fail("SOCKS curl failed");
        failed += 1;
    }

    println!();
    println!("Packet capture test:");

    match capture_and_check(input, &host, interface) {
        Ok(CaptureResult {
            sni_names,
            destinations,
            suspicious_sni,
            suspicious_dst,
        }) => {
            if destinations.is_empty() {
                warn("no TCP SYN destinations captured");
            } else {
                println!("Captured destinations:");
                for dst in &destinations {
                    println!("  {}", dst);
                }
            }

            if sni_names.is_empty() {
                warn("no TLS SNI captured");
            } else {
                println!("Captured SNI:");
                for sni in &sni_names {
                    println!("  {}", sni);
                }
            }

            if suspicious_dst.is_empty() {
                ok("all captured destinations are known proxy nodes");
            } else {
                fail("captured unknown destinations");
                for dst in suspicious_dst {
                    println!("  unknown: {}", dst);
                }
                failed += 1;
            }

            if suspicious_sni.is_empty() {
                ok("real target domain was not visible in SNI");
            } else {
                fail("real target domain appeared in SNI");
                for sni in suspicious_sni {
                    println!("  leaked SNI: {}", sni);
                }
                failed += 1;
            }
        }
        Err(err) => {
            warn(&format!("capture test skipped/failed: {:#}", err));
            warn("Install tcpdump + tshark and run leak-test with sudo for full packet checks.");
        }
    }

    println!();
    println!("Result:");
    if failed == 0 {
        ok("no obvious leaks detected");
        Ok(())
    } else {
        anyhow::bail!("leak-test failed: {} problem(s) found", failed);
    }
}

struct CaptureResult {
    sni_names: BTreeSet<String>,
    destinations: BTreeSet<String>,
    suspicious_sni: Vec<String>,
    suspicious_dst: Vec<String>,
}

fn capture_and_check(input: &Path, host: &str, interface: Option<&str>) -> Result<CaptureResult> {
    require_command("tcpdump")?;
    require_command("tshark")?;

    let config = load_config(input)?;

    let iface = match interface {
        Some(iface) => iface.to_string(),
        None => detect_default_interface()?,
    };

    let allowed = allowed_destinations(&config);

    if allowed.is_empty() {
        anyhow::bail!("no IP proxy nodes found in config");
    }

    let pcap = temp_pcap_path();

    println!("Interface: {}", iface);
    println!("PCAP: {}", pcap.display());

    let filter = build_tcpdump_filter(&config);

    let mut child = Command::new("tcpdump")
        .args(["-i", &iface, "-nn", "-s0", "-w"])
        .arg(&pcap)
        .arg(&filter)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to start tcpdump")?;

    thread::sleep(Duration::from_millis(800));

    let url = format!("https://{}", host);
    let socks = format!("{}:{}", config.general.listen, config.general.listen_port);
    let _ = curl_head(&url, Some(&socks), 15);

    thread::sleep(Duration::from_millis(1200));

    let _ = child.kill();
    let _ = child.wait();

    let sni_names = tshark_sni(&pcap)?;
    let destinations = tshark_destinations(&pcap)?;

    let suspicious_sni = sni_names
        .iter()
        .filter(|sni| is_same_or_subdomain(sni, host))
        .cloned()
        .collect::<Vec<_>>();

    let suspicious_dst = destinations
        .iter()
        .filter(|dst| !allowed.contains(*dst))
        .cloned()
        .collect::<Vec<_>>();

    Ok(CaptureResult {
        sni_names,
        destinations,
        suspicious_sni,
        suspicious_dst,
    })
}

fn allowed_destinations(config: &crate::config::SmartRouteConfig) -> HashSet<String> {
    let mut allowed = HashSet::new();

    for node in &config.nodes {
        if node.server.parse::<IpAddr>().is_ok() {
            allowed.insert(format!("{}\t{}", node.server, node.port));
        }
    }

    allowed
}

fn build_tcpdump_filter(config: &crate::config::SmartRouteConfig) -> String {
    let mut ports = config
        .nodes
        .iter()
        .map(|node| node.port)
        .collect::<Vec<_>>();

    ports.sort();
    ports.dedup();

    if ports.is_empty() {
        return "tcp".to_string();
    }

    let parts = ports
        .iter()
        .map(|port| format!("tcp port {}", port))
        .collect::<Vec<_>>();

    parts.join(" or ")
}

fn tshark_sni(pcap: &Path) -> Result<BTreeSet<String>> {
    let output = Command::new("tshark")
        .arg("-r")
        .arg(pcap)
        .args([
            "-Y",
            "tls.handshake.extensions_server_name",
            "-T",
            "fields",
            "-e",
            "tls.handshake.extensions_server_name",
        ])
        .output()
        .context("failed to run tshark SNI check")?;

    let text = String::from_utf8_lossy(&output.stdout);

    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect())
}

fn tshark_destinations(pcap: &Path) -> Result<BTreeSet<String>> {
    let output = Command::new("tshark")
        .arg("-r")
        .arg(pcap)
        .args([
            "-Y",
            "tcp.flags.syn == 1 && tcp.flags.ack == 0",
            "-T",
            "fields",
            "-e",
            "ip.dst",
            "-e",
            "tcp.dstport",
        ])
        .output()
        .context("failed to run tshark destination check")?;

    let text = String::from_utf8_lossy(&output.stdout);

    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect())
}

fn curl_head(url: &str, socks: Option<&str>, max_time: u64) -> Result<bool> {
    let mut cmd = Command::new("curl");

    if let Some(socks) = socks {
        cmd.args(["--socks5-hostname", socks]);
    }

    let output = cmd
        .args([
            "-I",
            url,
            "--max-time",
            &max_time.to_string(),
            "-sS",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
        ])
        .output()
        .context("failed to run curl")?;

    if output.status.success() {
        let code = String::from_utf8_lossy(&output.stdout);
        println!("  curl {} -> HTTP {}", url, code.trim());
        Ok(true)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        println!("  curl {} -> blocked/failed ({})", url, stderr.trim());
        Ok(false)
    }
}

fn check_killswitch_enabled() -> bool {
    Command::new("nft")
        .args(["list", "table", "inet", "smartroute"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn detect_default_interface() -> Result<String> {
    let output = Command::new("ip")
        .args(["route", "get", "1.1.1.1"])
        .output()
        .context("failed to detect default interface")?;

    let text = String::from_utf8_lossy(&output.stdout);
    let mut prev = "";

    for part in text.split_whitespace() {
        if prev == "dev" {
            return Ok(part.to_string());
        }

        prev = part;
    }

    anyhow::bail!("could not detect default network interface");
}

fn require_command(name: &str) -> Result<()> {
    let status = Command::new(name)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .with_context(|| format!("{} not found", name))?;

    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("{} returned error", name);
    }
}

fn temp_pcap_path() -> PathBuf {
    let pid = std::process::id();
    PathBuf::from(format!("/tmp/smartroute-leak-test-{}.pcap", pid))
}

fn normalize_host(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split('/')
        .next()
        .unwrap_or(value)
        .split(':')
        .next()
        .unwrap_or(value)
        .to_string()
}

fn is_same_or_subdomain(sni: &str, host: &str) -> bool {
    sni == host || sni.ends_with(&format!(".{}", host))
}

fn ok(message: &str) {
    println!("[OK] {}", message);
}

fn warn(message: &str) {
    println!("[WARN] {}", message);
}

fn fail(message: &str) {
    println!("[FAIL] {}", message);
}
