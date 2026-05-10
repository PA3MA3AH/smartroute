use anyhow::{Context, Result};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(not(windows))]
const SERVICE_PATH: &str = "/etc/systemd/system/smartroute.service";

#[cfg(windows)]
const TASK_NAME: &str = "SmartRoute";

fn current_exe_path() -> Result<PathBuf> {
    env::current_exe().context("Failed to detect current executable path")
}

pub fn enable_autostart(config_path: &Path) -> Result<()> {
    enable_autostart_impl(config_path)
}

pub fn disable_autostart() -> Result<()> {
    disable_autostart_impl()
}

pub fn status_autostart() -> Result<()> {
    status_autostart_impl()
}

#[cfg(not(windows))]
fn enable_autostart_impl(config_path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let exe = current_exe_path()?;
    let config = fs::canonicalize(config_path)
        .with_context(|| format!("Config not found: {}", config_path.display()))?;

    let service = format!(
        r#"[Unit]
Description=SmartRoute proxy router
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart={} daemon {} --diagnose-interval 300 --timeout 8 --jobs 12 --samples 3 --hysteresis 25
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
"#,
        exe.display(),
        config.display()
    );

    fs::write(SERVICE_PATH, service)
        .with_context(|| format!("Failed to write {}", SERVICE_PATH))?;

    let _ = fs::set_permissions(SERVICE_PATH, fs::Permissions::from_mode(0o644));

    run("systemctl", &["daemon-reload"])?;
    run("systemctl", &["enable", "--now", "smartroute.service"])?;

    println!("Autostart enabled: {}", SERVICE_PATH);
    Ok(())
}

#[cfg(windows)]
fn enable_autostart_impl(config_path: &Path) -> Result<()> {
    let exe = current_exe_path()?;
    let config = fs::canonicalize(config_path)
        .with_context(|| format!("Config not found: {}", config_path.display()))?;

    let task_run = format!(
        "\"{}\" daemon \"{}\" --diagnose-interval 300 --timeout 8 --jobs 12 --samples 3 --hysteresis 25",
        exe.display(),
        config.display()
    );

    let status = Command::new("schtasks")
        .args([
            "/Create",
            "/TN",
            TASK_NAME,
            "/TR",
            &task_run,
            "/SC",
            "ONLOGON",
            "/RL",
            "HIGHEST",
            "/F",
        ])
        .status()
        .context("Failed to create Windows Scheduled Task with schtasks")?;

    if !status.success() {
        anyhow::bail!("schtasks failed to create autostart task");
    }

    println!("Autostart enabled: Windows Scheduled Task '{}'", TASK_NAME);
    println!("Task command: {}", task_run);
    Ok(())
}

#[cfg(not(windows))]
fn disable_autostart_impl() -> Result<()> {
    let _ = Command::new("systemctl")
        .args(["disable", "--now", "smartroute.service"])
        .status();

    let _ = fs::remove_file(SERVICE_PATH);

    run("systemctl", &["daemon-reload"])?;

    println!("Autostart disabled");
    Ok(())
}

#[cfg(windows)]
fn disable_autostart_impl() -> Result<()> {
    let status = Command::new("schtasks")
        .args(["/Delete", "/TN", TASK_NAME, "/F"])
        .status()
        .context("Failed to delete Windows Scheduled Task with schtasks")?;

    if status.success() {
        println!("Autostart disabled");
    } else {
        println!("Autostart was probably already disabled");
    }

    Ok(())
}

#[cfg(not(windows))]
fn status_autostart_impl() -> Result<()> {
    let enabled = Command::new("systemctl")
        .args(["is-enabled", "smartroute.service"])
        .output()
        .context("Failed to check autostart status")?;

    let active = Command::new("systemctl")
        .args(["is-active", "smartroute.service"])
        .output()
        .context("Failed to check service status")?;

    println!(
        "Autostart: {}",
        String::from_utf8_lossy(&enabled.stdout).trim()
    );
    println!(
        "Service: {}",
        String::from_utf8_lossy(&active.stdout).trim()
    );

    Ok(())
}

#[cfg(windows)]
fn status_autostart_impl() -> Result<()> {
    let output = Command::new("schtasks")
        .args(["/Query", "/TN", TASK_NAME])
        .output()
        .context("Failed to query Windows Scheduled Task with schtasks")?;

    if output.status.success() {
        println!("Autostart: enabled");
        println!("{}", String::from_utf8_lossy(&output.stdout).trim());
    } else {
        println!("Autostart: disabled");
    }

    Ok(())
}

#[cfg(not(windows))]
fn run(cmd: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(cmd)
        .args(args)
        .status()
        .with_context(|| format!("Failed to run {}", cmd))?;

    if !status.success() {
        anyhow::bail!("Command failed: {} {}", cmd, args.join(" "));
    }

    Ok(())
}
