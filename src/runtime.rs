use crate::{
    config::{load_config, validate_config},
    resolve::resolve_domains_to_ip,
    singbox::generate_singbox_config,
};
use anyhow::{Context, Result};
use std::{
    fs,
    net::{TcpStream, ToSocketAddrs},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

pub const RUNTIME_DIR: &str = "/run/smartroute";
pub const PID_FILE: &str = "/run/smartroute/smartroute.pid";
pub const LOG_FILE: &str = "/run/smartroute/sing-box.log";
pub const SINGBOX_CONFIG_FILE: &str = "/run/smartroute/sing-box.json";

const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const POLL_INTERVAL: Duration = Duration::from_millis(100);

fn wait_for_port(host: &str, port: u16, timeout: Duration) -> Result<()> {
    let addr = format!("{}:{}", host, port);
    let start = std::time::Instant::now();

    loop {
        if start.elapsed() > timeout {
            anyhow::bail!(
                "Timeout waiting for {}:{} to become available after {:?}",
                host,
                port,
                timeout
            );
        }

        if let Ok(mut addrs) = addr.to_socket_addrs() {
            if let Some(socket_addr) = addrs.next() {
                if TcpStream::connect_timeout(&socket_addr, Duration::from_millis(500)).is_ok() {
                    return Ok(());
                }
            }
        }

        thread::sleep(POLL_INTERVAL);
    }
}

pub fn start_smartroute(input: &Path) -> Result<()> {
    if let Err(err) = resolve_domains_to_ip(input) {
        eprintln!("Warning: failed to resolve domains before start: {err:#}");
    }

    let config = load_config(input)?;
    validate_config(&config)?;

    fs::create_dir_all(RUNTIME_DIR).context("Failed to create /run/smartroute")?;

    if is_running()? {
        anyhow::bail!("SmartRoute is already running");
    }

    let _ = fs::remove_file(PID_FILE);

    let singbox_config = generate_singbox_config(&config)?;

    let pretty = serde_json::to_string_pretty(&singbox_config)
        .context("Failed to serialize sing-box config")?;

    fs::write(SINGBOX_CONFIG_FILE, pretty)
        .with_context(|| format!("Failed to write {}", SINGBOX_CONFIG_FILE))?;

    let log_file = fs::File::create(LOG_FILE).context("Failed to create sing-box log file")?;

    let mut child = Command::new("sing-box")
        .arg("run")
        .arg("-c")
        .arg(SINGBOX_CONFIG_FILE)
        .stdout(Stdio::from(log_file.try_clone()?))
        .stderr(Stdio::from(log_file))
        .spawn()
        .context("Failed to start sing-box. Is sing-box installed?")?;

    let pid = child.id();
    fs::write(PID_FILE, pid.to_string()).context("Failed to write PID file")?;

    // Wait for sing-box to become ready by checking if the port is available
    let wait_result = wait_for_port(&config.general.listen, config.general.listen_port, STARTUP_TIMEOUT);

    // Check if process is still running
    if let Some(status) = child
        .try_wait()
        .context("Failed to check sing-box process status")?
    {
        let log = fs::read_to_string(LOG_FILE).unwrap_or_default();
        let tail = last_lines(&log, 60);

        anyhow::bail!(
            "sing-box exited immediately with status: {}\nLast log lines:\n{}",
            status,
            tail
        );
    }

    // If port check failed but process is still running, report the error
    if let Err(e) = wait_result {
        let log = fs::read_to_string(LOG_FILE).unwrap_or_default();
        let tail = last_lines(&log, 60);

        anyhow::bail!(
            "sing-box started but failed to bind to {}:{}\nError: {}\nLast log lines:\n{}",
            config.general.listen,
            config.general.listen_port,
            e,
            tail
        );
    }

    println!("SmartRoute started with PID {}", pid);
    println!(
        "Mode: {} on {}:{}",
        config.general.mode, config.general.listen, config.general.listen_port
    );
    println!("Config: {}", SINGBOX_CONFIG_FILE);
    println!("Log: {}", LOG_FILE);

    Ok(())
}

pub fn stop_smartroute() -> Result<()> {
    let pid = match fs::read_to_string(PID_FILE) {
        Ok(pid) => pid.trim().to_string(),
        Err(_) => {
            println!("SmartRoute is not running");
            return Ok(());
        }
    };

    let running = Command::new("kill")
        .arg("-0")
        .arg(&pid)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("Failed to check SmartRoute process")?
        .success();

    if running {
        let status = Command::new("kill")
            .arg(&pid)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .context("Failed to stop SmartRoute")?;

        if status.success() {
            println!("SmartRoute stopped");
        } else {
            println!("SmartRoute process could not be stopped");
        }
    } else {
        println!("SmartRoute process was not running, stale PID file removed");
    }

    let _ = fs::remove_file(PID_FILE);

    Ok(())
}

pub fn status_smartroute() -> Result<()> {
    match fs::read_to_string(PID_FILE) {
        Ok(pid) => {
            let pid = pid.trim();

            let status = Command::new("kill")
                .arg("-0")
                .arg(pid)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .context("Failed to check SmartRoute status")?;

            if status.success() {
                println!("SmartRoute is running, PID {}", pid);
                println!("Log: {}", LOG_FILE);
            } else {
                println!("SmartRoute PID file exists, but process is not running");
                println!("Try: sudo rm -f {}", PID_FILE);
            }
        }
        Err(_) => {
            println!("SmartRoute is not running");
        }
    }

    Ok(())
}

fn is_running() -> Result<bool> {
    let Ok(pid) = fs::read_to_string(PID_FILE) else {
        return Ok(false);
    };

    let pid = pid.trim();

    let status = Command::new("kill")
        .arg("-0")
        .arg(pid)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("Failed to check old SmartRoute process")?;

    if !status.success() {
        let _ = fs::remove_file(PID_FILE);
    }

    Ok(status.success())
}

fn last_lines(text: &str, max_lines: usize) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(max_lines);

    lines[start..].join("\n")
}
