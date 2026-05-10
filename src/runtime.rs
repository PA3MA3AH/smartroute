use crate::{
    config::{load_config, validate_config},
    platform,
    resolve::resolve_domains_to_ip,
    singbox::generate_singbox_config,
};
use anyhow::{Context, Result};
use std::{
    fs,
    net::{TcpStream, ToSocketAddrs},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

pub const RUNTIME_DIR: &str = if cfg!(windows) {
    "%PROGRAMDATA%\\SmartRoute\\run"
} else {
    "/run/smartroute"
};
pub const PID_FILE: &str = if cfg!(windows) {
    "%PROGRAMDATA%\\SmartRoute\\run\\smartroute.pid"
} else {
    "/run/smartroute/smartroute.pid"
};
pub const LOG_FILE: &str = if cfg!(windows) {
    "%PROGRAMDATA%\\SmartRoute\\run\\sing-box.log"
} else {
    "/run/smartroute/sing-box.log"
};
pub const SINGBOX_CONFIG_FILE: &str = if cfg!(windows) {
    "%PROGRAMDATA%\\SmartRoute\\run\\sing-box.json"
} else {
    "/run/smartroute/sing-box.json"
};

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
        tracing::warn!(error = %err, "Failed to resolve domains before start");
    }

    let config = load_config(input)?;
    validate_config(&config)?;

    let runtime_dir = platform::ensure_runtime_dir()?;
    let pid_file = platform::pid_file();
    let log_file_path = platform::log_file();
    let singbox_config_file = platform::singbox_config_file();

    if is_running()? {
        anyhow::bail!("SmartRoute is already running");
    }

    let _ = fs::remove_file(&pid_file);

    tracing::info!("Generating sing-box configuration");
    let singbox_config = generate_singbox_config(&config)?;

    let pretty = serde_json::to_string_pretty(&singbox_config)
        .context("Failed to serialize sing-box config")?;

    crate::util::atomic_write(&singbox_config_file, &pretty)
        .with_context(|| format!("Failed to write {}", singbox_config_file.display()))?;

    let log_file = fs::File::create(&log_file_path)
        .with_context(|| format!("Failed to create sing-box log file: {}", log_file_path.display()))?;

    let singbox_bin = platform::singbox_bin();

    tracing::info!(
        binary = %singbox_bin,
        runtime_dir = %runtime_dir.display(),
        "Starting sing-box process"
    );

    let mut child = Command::new(&singbox_bin)
        .arg("run")
        .arg("-c")
        .arg(&singbox_config_file)
        .stdout(Stdio::from(log_file.try_clone()?))
        .stderr(Stdio::from(log_file))
        .spawn()
        .with_context(|| format!("Failed to start {}. Is sing-box installed and in PATH?", singbox_bin))?;

    let pid = child.id();
    fs::write(&pid_file, pid.to_string())
        .with_context(|| format!("Failed to write PID file: {}", pid_file.display()))?;

    tracing::debug!(
        pid = %pid,
        host = %config.general.listen,
        port = %config.general.listen_port,
        timeout_secs = %STARTUP_TIMEOUT.as_secs(),
        "Waiting for sing-box to become ready"
    );

    let wait_result = wait_for_port(&config.general.listen, config.general.listen_port, STARTUP_TIMEOUT);

    if let Some(status) = child
        .try_wait()
        .context("Failed to check sing-box process status")?
    {
        let log = fs::read_to_string(&log_file_path).unwrap_or_default();
        let tail = last_lines(&log, 60);

        tracing::error!(
            status = %status,
            log_tail = %tail,
            "sing-box exited immediately"
        );

        let _ = fs::remove_file(&pid_file);

        anyhow::bail!(
            "sing-box exited immediately with status: {}\nLast log lines:\n{}",
            status,
            tail
        );
    }

    if let Err(e) = wait_result {
        let log = fs::read_to_string(&log_file_path).unwrap_or_default();
        let tail = last_lines(&log, 60);

        tracing::error!(
            error = %e,
            host = %config.general.listen,
            port = %config.general.listen_port,
            log_tail = %tail,
            "sing-box started but failed to bind to port"
        );

        anyhow::bail!(
            "sing-box started but failed to bind to {}:{}\nError: {}\nLast log lines:\n{}",
            config.general.listen,
            config.general.listen_port,
            e,
            tail
        );
    }

    tracing::info!(
        pid = %pid,
        mode = %config.general.mode,
        listen = %format!("{}:{}", config.general.listen, config.general.listen_port),
        config_file = %singbox_config_file.display(),
        log_file = %log_file_path.display(),
        "SmartRoute started successfully"
    );

    Ok(())
}

pub fn stop_smartroute() -> Result<()> {
    let pid_file = platform::pid_file();

    let pid = match read_pid(&pid_file) {
        Ok(pid) => pid,
        Err(_) => {
            tracing::info!("SmartRoute is not running (no PID file)");
            return Ok(());
        }
    };

    if platform::process_exists(pid)? {
        tracing::info!(pid = %pid, "Stopping SmartRoute");

        if platform::stop_process(pid)? {
            tracing::info!(pid = %pid, "SmartRoute stopped successfully");
        } else {
            tracing::error!(pid = %pid, "Failed to stop SmartRoute process");
        }
    } else {
        tracing::warn!(pid = %pid, "SmartRoute process was not running, removing stale PID file");
    }

    let _ = fs::remove_file(&pid_file);

    Ok(())
}

pub fn status_smartroute() -> Result<()> {
    let pid_file = platform::pid_file();
    let log_file = platform::log_file();

    match read_pid(&pid_file) {
        Ok(pid) => {
            if platform::process_exists(pid)? {
                tracing::info!(
                    pid = %pid,
                    log_file = %log_file.display(),
                    "SmartRoute is running"
                );
            } else {
                tracing::warn!(
                    pid = %pid,
                    pid_file = %pid_file.display(),
                    "SmartRoute PID file exists, but process is not running"
                );
            }
        }
        Err(_) => {
            tracing::info!("SmartRoute is not running");
        }
    }

    Ok(())
}

fn is_running() -> Result<bool> {
    let pid_file = platform::pid_file();

    let Ok(pid) = read_pid(&pid_file) else {
        return Ok(false);
    };

    let running = platform::process_exists(pid)?;

    if !running {
        let _ = fs::remove_file(&pid_file);
    }

    Ok(running)
}

fn read_pid(path: &Path) -> Result<u32> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Failed to read PID file: {}", path.display()))?;

    raw.trim()
        .parse::<u32>()
        .with_context(|| format!("Invalid PID in {}", path.display()))
}

fn last_lines(text: &str, max_lines: usize) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(max_lines);

    lines[start..].join("\n")
}
