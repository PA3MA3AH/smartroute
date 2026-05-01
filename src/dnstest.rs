use crate::config::{SmartRouteConfig, load_config};
use anyhow::{Context, Result};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

const KNOWN_DOH_SNI: &[&str] = &[
    "dns.google",
    "dns.google.com",
    "cloudflare-dns.com",
    "one.one.one.one",
    "dns.quad9.net",
    "quad9.net",
    "dns.adguard-dns.com",
    "dns.nextdns.io",
    "doh.opendns.com",
    "mozilla.cloudflare-dns.com",
];

pub fn run_dns_test(
    input: &Path,
    domain: &str,
    interface: Option<&str>,
    strict: bool,
) -> Result<()> {
    let config = load_config(input)?;
    let host = normalize_host(domain);
    let url = format!("https://{}", host);

    println!("SmartRoute DNS leak-test");
    println!("────────────────────────────────────────────────────────");
    println!("Config: {}", input.display());
    println!("Test domain: {}", host);
    println!(
        "SOCKS5: {}:{}",
        config.general.listen, config.general.listen_port
    );
    println!("Strict mode: {}", strict);
    println!();

    require_command("tcpdump")?;
    require_command("tshark")?;
    require_command("curl")?;

    let iface = match interface {
        Some(iface) => iface.to_string(),
        None => detect_default_interface()?,
    };

    let pcap = temp_pcap_path();

    println!("Interface: {}", iface);
    println!("PCAP: {}", pcap.display());

    let filter = "udp port 53 or tcp port 53 or tcp port 853 or tcp port 443";

    let mut child = Command::new("tcpdump")
        .args(["-i", &iface, "-nn", "-s0", "-w"])
        .arg(&pcap)
        .arg(filter)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("failed to start tcpdump. Run dns-test with sudo.")?;

    thread::sleep(Duration::from_millis(800));

    let socks = format!("{}:{}", config.general.listen, config.general.listen_port);
    let socks_ok = curl_head(&url, Some(&socks), 15)?;

    thread::sleep(Duration::from_millis(1200));

    let _ = child.kill();
    let _ = child.wait();

    let dns_queries = tshark_dns_queries(&pcap)?;
    let dot_destinations = tshark_dot_destinations(&pcap)?;
    let sni_names = tshark_tls_sni(&pcap)?;

    println!();

    let mut failed = 0usize;

    if socks_ok {
        ok("SOCKS curl works through SmartRoute");
    } else {
        fail("SOCKS curl failed");
        failed += 1;
    }

    println!();
    println!("Captured DNS queries:");

    if dns_queries.is_empty() {
        ok("no DNS queries captured on UDP/TCP 53");
    } else {
        for query in &dns_queries {
            println!("  {}", query);
        }

        let target_dns = dns_queries
            .iter()
            .filter(|query| query_mentions_host(query, &host))
            .cloned()
            .collect::<Vec<_>>();

        if target_dns.is_empty() {
            warn("DNS queries were captured, but not for target domain");
            if strict {
                fail("strict mode: any direct DNS query is considered a leak");
                failed += 1;
            }
        } else {
            fail("target domain appeared in direct DNS queries");
            for query in target_dns {
                println!("  leaked DNS: {}", query);
            }
            failed += 1;
        }
    }

    println!();
    println!("Captured DoT destinations:");

    if dot_destinations.is_empty() {
        ok("no DNS-over-TLS connections captured on TCP/853");
    } else {
        fail("DNS-over-TLS traffic detected");
        for dst in &dot_destinations {
            println!("  DoT: {}", dst);
        }
        failed += 1;
    }

    println!();
    println!("Captured TLS SNI:");

    if sni_names.is_empty() {
        ok("no TLS SNI captured in DNS test window");
    } else {
        for sni in &sni_names {
            println!("  {}", sni);
        }

        let target_sni = sni_names
            .iter()
            .filter(|sni| is_same_or_subdomain(sni, &host))
            .cloned()
            .collect::<Vec<_>>();

        let doh_sni = sni_names
            .iter()
            .filter(|sni| is_known_doh_sni(sni))
            .cloned()
            .collect::<Vec<_>>();

        if target_sni.is_empty() {
            ok("target domain was not visible in TLS SNI");
        } else {
            fail("target domain appeared in TLS SNI");
            for sni in target_sni {
                println!("  leaked SNI: {}", sni);
            }
            failed += 1;
        }

        if doh_sni.is_empty() {
            ok("known DoH SNI was not captured");
        } else {
            fail("known DNS-over-HTTPS SNI detected");
            for sni in doh_sni {
                println!("  DoH SNI: {}", sni);
            }
            failed += 1;
        }
    }

    println!();
    print_dns_policy_hint(&config);

    println!();
    println!("Result:");

    if failed == 0 {
        ok("no obvious DNS leaks detected");
        Ok(())
    } else {
        anyhow::bail!("dns-test failed: {} problem(s) found", failed);
    }
}

fn tshark_dns_queries(pcap: &Path) -> Result<BTreeSet<String>> {
    let output = Command::new("tshark")
        .arg("-r")
        .arg(pcap)
        .args([
            "-Y",
            "dns.qry.name",
            "-T",
            "fields",
            "-e",
            "ip.dst",
            "-e",
            "udp.dstport",
            "-e",
            "tcp.dstport",
            "-e",
            "dns.qry.name",
        ])
        .output()
        .context("failed to run tshark DNS query check")?;

    let text = String::from_utf8_lossy(&output.stdout);

    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect())
}

fn tshark_dot_destinations(pcap: &Path) -> Result<BTreeSet<String>> {
    let output = Command::new("tshark")
        .arg("-r")
        .arg(pcap)
        .args([
            "-Y",
            "tcp.dstport == 853 && tcp.flags.syn == 1 && tcp.flags.ack == 0",
            "-T",
            "fields",
            "-e",
            "ip.dst",
            "-e",
            "tcp.dstport",
        ])
        .output()
        .context("failed to run tshark DoT check")?;

    let text = String::from_utf8_lossy(&output.stdout);

    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect())
}

fn tshark_tls_sni(pcap: &Path) -> Result<BTreeSet<String>> {
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
        .context("failed to run tshark TLS SNI check")?;

    let text = String::from_utf8_lossy(&output.stdout);

    Ok(text
        .lines()
        .flat_map(|line| line.split(','))
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
    PathBuf::from(format!(
        "/tmp/smartroute-dns-test-{}.pcap",
        std::process::id()
    ))
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

fn query_mentions_host(query_line: &str, host: &str) -> bool {
    query_line
        .split_whitespace()
        .any(|part| is_same_or_subdomain(part.trim_end_matches(','), host))
}

fn is_same_or_subdomain(name: &str, base: &str) -> bool {
    let name = name.trim().trim_end_matches('.').to_ascii_lowercase();
    let base = base.trim().trim_end_matches('.').to_ascii_lowercase();

    name == base || name.ends_with(&format!(".{}", base))
}

fn is_known_doh_sni(sni: &str) -> bool {
    KNOWN_DOH_SNI
        .iter()
        .any(|known| is_same_or_subdomain(sni, known))
}

fn print_dns_policy_hint(config: &SmartRouteConfig) {
    println!("DNS policy hint:");
    println!(
        "  SmartRoute mode: {}, SOCKS: {}:{}",
        config.general.mode, config.general.listen, config.general.listen_port
    );
    println!("  Best result: no UDP/TCP 53, no TCP/853, no DoH SNI, target domain not visible.");
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
