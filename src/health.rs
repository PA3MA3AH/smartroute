use crate::{
    config::{SmartRouteConfig, load_config, validate_config},
    killswitch::enable_killswitch,
    resolve::resolve_domains_to_ip,
    runtime::{LOG_FILE, PID_FILE, start_smartroute, stop_smartroute},
    singbox::generate_singbox_config,
};
use anyhow::{Context, Result};
use std::{
    fs,
    io::Write,
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

pub fn health_check(input: &Path, domain: &str, full: bool) -> Result<()> {
    let config = load_config(input)?;

    println!("SmartRoute health check");
    println!("────────────────────────────────────────────────────────");
    println!("Config: {}", input.display());
    println!(
        "SOCKS5: {}:{}",
        config.general.listen, config.general.listen_port
    );
    println!("Full network checks: {}", full);
    println!();

    let mut failed = 0usize;

    match validate_config(&config) {
        Ok(_) => ok("SmartRoute config is valid"),
        Err(err) => {
            fail(&format!("config validation failed: {err:#}"));
            failed += 1;
        }
    }

    match check_generated_singbox_config(&config) {
        Ok(_) => ok("generated sing-box config is valid"),
        Err(err) => {
            fail(&format!("generated sing-box config is invalid: {err:#}"));
            failed += 1;
        }
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

    let direct_rules = count_direct_rules(&config);

    if direct_rules == 0 {
        ok("no rules use outbound = direct");
    } else {
        fail(&format!("{} rule(s) use outbound = direct", direct_rules));
        failed += 1;
    }

    if pid_file_process_is_running()? {
        ok("managed sing-box process is running");
    } else if singbox_process_exists() {
        warn("sing-box exists, but PID file is missing/stale");
    } else {
        fail("sing-box is not running");
        failed += 1;
    }

    if socks_is_listening(&config)? {
        ok("SOCKS port is listening");
    } else {
        fail("SOCKS port is not listening");
        failed += 1;
    }

    if killswitch_is_enabled() {
        ok("kill-switch is enabled");
    } else {
        fail("kill-switch is disabled");
        failed += 1;
    }

    if full {
        let host = normalize_host(domain);
        let url = format!("https://{}", host);
        let socks = format!("{}:{}", config.general.listen, config.general.listen_port);

        println!();
        println!("Network checks:");

        if curl_head(&url, None, 5)? {
            fail("direct curl succeeded, this is a leak");
            failed += 1;
        } else {
            ok("direct curl is blocked");
        }

        if curl_head(&url, Some(&socks), 15)? {
            ok("SOCKS curl works through SmartRoute");
        } else {
            fail("SOCKS curl failed");
            failed += 1;
        }
    }

    println!();
    println!("Result:");

    if failed == 0 {
        ok("SmartRoute looks healthy");
        Ok(())
    } else {
        anyhow::bail!("health check failed: {} problem(s) found", failed);
    }
}

pub fn repair_smartroute(input: &Path, domain: &str, full: bool) -> Result<()> {
    println!("SmartRoute repair");
    println!("────────────────────────────────────────────────────────");
    println!("Config: {}", input.display());
    println!();

    match resolve_domains_to_ip(input) {
        Ok(changed) => {
            if changed > 0 {
                ok(&format!("resolved {} domain node(s) to IP", changed));
            } else {
                ok("proxy nodes already use IP addresses");
            }
        }
        Err(err) => warn(&format!("domain resolver failed: {err:#}")),
    }

    let config = load_config(input)?;
    validate_config(&config)?;

    if config.general.final_outbound == "direct" {
        anyhow::bail!("Cannot repair automatically: final_outbound is direct");
    }

    let direct_rules = count_direct_rules(&config);
    if direct_rules > 0 {
        anyhow::bail!(
            "Cannot repair automatically: {} rule(s) use outbound = direct",
            direct_rules
        );
    }

    check_generated_singbox_config(&config)?;

    let pid_running = pid_file_process_is_running()?;
    let socks_listening = socks_is_listening(&config)?;

    if !pid_running && socks_listening {
        if adopt_existing_singbox_if_possible()? {
            ok("adopted existing sing-box process into PID file");
        } else {
            warn("SOCKS is listening, but SmartRoute PID file is missing/stale");
        }
    }

    let pid_running = pid_file_process_is_running()?;
    let socks_listening = socks_is_listening(&config)?;

    if !pid_running || !socks_listening {
        warn("SmartRoute runtime is broken, restarting...");
        let _ = stop_smartroute();
        start_smartroute(input)?;
        ok("SmartRoute restarted");
    } else {
        ok("SmartRoute runtime is already running");
    }

    if !killswitch_is_enabled() {
        warn("kill-switch is disabled, enabling...");
        enable_killswitch(input, true)?;
        ok("kill-switch enabled");
    } else {
        ok("kill-switch already enabled");
    }

    if full {
        let host = normalize_host(domain);
        let url = format!("https://{}", host);
        let socks = format!("{}:{}", config.general.listen, config.general.listen_port);

        if !curl_head(&url, Some(&socks), 15)? {
            warn("SOCKS curl failed after repair, restarting once more...");
            let _ = stop_smartroute();
            start_smartroute(input)?;

            if !curl_head(&url, Some(&socks), 15)? {
                anyhow::bail!("SOCKS still does not work after repair");
            }
        }

        ok("SOCKS network check works");
    }

    println!();
    ok("repair finished");

    Ok(())
}

pub fn daemon_self_heal(input: &Path) -> Result<()> {
    let config = load_config(input)?;

    validate_config(&config)?;
    check_generated_singbox_config(&config)?;

    let pid_running = pid_file_process_is_running()?;
    let socks_listening = socks_is_listening(&config)?;

    if !pid_running && socks_listening {
        let _ = adopt_existing_singbox_if_possible();
    }

    let pid_running = pid_file_process_is_running()?;
    let socks_listening = socks_is_listening(&config)?;

    if !pid_running || !socks_listening {
        eprintln!("Self-heal: SmartRoute runtime is broken, restarting...");
        let _ = stop_smartroute();
        start_smartroute(input)?;
    }

    if !killswitch_is_enabled() {
        eprintln!("Self-heal: kill-switch is disabled, enabling...");
        enable_killswitch(input, true)?;
    }

    Ok(())
}

fn check_generated_singbox_config(config: &SmartRouteConfig) -> Result<()> {
    let value = generate_singbox_config(config)?;
    let raw = serde_json::to_string_pretty(&value)?;

    let path = temp_singbox_config_path();
    fs::write(&path, raw).with_context(|| format!("Failed to write {}", path.display()))?;

    let output = Command::new("sing-box")
        .arg("check")
        .arg("-c")
        .arg(&path)
        .output()
        .context("Failed to run sing-box check")?;

    let _ = fs::remove_file(&path);

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("{}", stderr.trim());
    }
}

fn pid_file_process_is_running() -> Result<bool> {
    let Ok(pid) = fs::read_to_string(PID_FILE) else {
        return Ok(false);
    };

    let pid = pid.trim();

    if pid.is_empty() {
        return Ok(false);
    }

    let status = Command::new("kill")
        .arg("-0")
        .arg(pid)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("Failed to check PID")?;

    if !status.success() {
        let _ = fs::remove_file(PID_FILE);
    }

    Ok(status.success())
}

fn singbox_process_exists() -> bool {
    Command::new("pgrep")
        .arg("-x")
        .arg("sing-box")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn adopt_existing_singbox_if_possible() -> Result<bool> {
    let output = Command::new("pgrep")
        .arg("-x")
        .arg("sing-box")
        .output()
        .context("Failed to run pgrep")?;

    if !output.status.success() {
        return Ok(false);
    }

    let output_text = String::from_utf8_lossy(&output.stdout);

    let pids = output_text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    if pids.len() != 1 {
        return Ok(false);
    }

    fs::write(PID_FILE, pids[0]).context("Failed to write SmartRoute PID file")?;

    Ok(true)
}

fn socks_is_listening(config: &SmartRouteConfig) -> Result<bool> {
    let addr = format!("{}:{}", config.general.listen, config.general.listen_port);

    let Some(socket_addr) = addr.to_socket_addrs()?.next() else {
        return Ok(false);
    };

    Ok(TcpStream::connect_timeout(&socket_addr, Duration::from_millis(1200)).is_ok())
}

fn killswitch_is_enabled() -> bool {
    Command::new("nft")
        .args(["list", "table", "inet", "smartroute"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn count_direct_rules(config: &SmartRouteConfig) -> usize {
    config
        .rules
        .iter()
        .filter(|rule| rule.outbound == "direct")
        .count()
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
        .context("Failed to run curl")?;

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

fn temp_singbox_config_path() -> PathBuf {
    PathBuf::from(format!(
        "/tmp/smartroute-health-singbox-{}.json",
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

fn ok(message: &str) {
    println!("[OK] {}", message);
}

fn warn(message: &str) {
    println!("[WARN] {}", message);
}

fn fail(message: &str) {
    println!("[FAIL] {}", message);
}

#[allow(dead_code)]
fn tail_log_hint() {
    let _ = fs::File::options()
        .create(true)
        .append(true)
        .open(LOG_FILE)
        .and_then(|mut file| writeln!(file, "SmartRoute health check touched log"));
}
