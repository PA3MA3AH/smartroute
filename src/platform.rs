use anyhow::{Context, Result};
use std::{
    env, fs,
    path::PathBuf,
    process::{Command, Stdio},
};

pub fn runtime_dir() -> PathBuf {
    if let Some(path) = env::var_os("SMARTROUTE_RUNTIME_DIR") {
        return PathBuf::from(path);
    }

    #[cfg(windows)]
    {
        if let Some(path) = env::var_os("PROGRAMDATA") {
            return PathBuf::from(path).join("SmartRoute").join("run");
        }

        if let Some(path) = env::var_os("LOCALAPPDATA") {
            return PathBuf::from(path).join("SmartRoute").join("run");
        }

        env::temp_dir().join("SmartRoute").join("run")
    }

    #[cfg(not(windows))]
    {
        PathBuf::from("/run/smartroute")
    }
}

pub fn pid_file() -> PathBuf {
    runtime_dir().join("smartroute.pid")
}

pub fn log_file() -> PathBuf {
    runtime_dir().join("sing-box.log")
}

pub fn singbox_config_file() -> PathBuf {
    runtime_dir().join("sing-box.json")
}

pub fn ensure_runtime_dir() -> Result<PathBuf> {
    let dir = runtime_dir();
    fs::create_dir_all(&dir).with_context(|| format!("Failed to create {}", dir.display()))?;
    Ok(dir)
}

pub fn singbox_bin() -> String {
    env::var("SMARTROUTE_SINGBOX").unwrap_or_else(|_| {
        if cfg!(windows) {
            "sing-box.exe".to_string()
        } else {
            "sing-box".to_string()
        }
    })
}

pub fn null_device() -> &'static str {
    if cfg!(windows) { "NUL" } else { "/dev/null" }
}

pub fn temp_file(stem: &str, extension: &str) -> PathBuf {
    let pid = std::process::id();

    let file_name = if extension.trim().is_empty() {
        format!("smartroute-{}-{}", stem, pid)
    } else {
        format!("smartroute-{}-{}.{}", stem, pid, extension.trim_start_matches('.'))
    };

    env::temp_dir().join(file_name)
}

pub fn process_exists(pid: u32) -> Result<bool> {
    process_exists_impl(pid)
}

pub fn stop_process(pid: u32) -> Result<bool> {
    stop_process_impl(pid)
}

pub fn process_name_exists(names: &[&str]) -> bool {
    names.iter().any(|name| process_name_exists_impl(name))
}

pub fn find_pids_by_name(name: &str) -> Result<Vec<u32>> {
    find_pids_by_name_impl(name)
}

#[cfg(not(windows))]
fn process_exists_impl(pid: u32) -> Result<bool> {
    let status = Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("Failed to check process")?;

    Ok(status.success())
}

#[cfg(windows)]
fn process_exists_impl(pid: u32) -> Result<bool> {
    let filter = format!("PID eq {}", pid);
    let output = Command::new("tasklist")
        .args(["/FI", &filter, "/FO", "CSV", "/NH"])
        .output()
        .context("Failed to run tasklist")?;

    if !output.status.success() {
        return Ok(false);
    }

    let text = String::from_utf8_lossy(&output.stdout);
    Ok(text.lines().any(|line| {
        line.split(',')
            .any(|field| field.trim_matches('"') == pid.to_string())
    }))
}

#[cfg(not(windows))]
fn stop_process_impl(pid: u32) -> Result<bool> {
    let status = Command::new("kill")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("Failed to stop process")?;

    Ok(status.success())
}

#[cfg(windows)]
fn stop_process_impl(pid: u32) -> Result<bool> {
    let status = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .context("Failed to run taskkill")?;

    Ok(status.success())
}

#[cfg(not(windows))]
fn process_name_exists_impl(name: &str) -> bool {
    Command::new("pgrep")
        .arg("-x")
        .arg(name)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn process_name_exists_impl(name: &str) -> bool {
    let filter = format!("IMAGENAME eq {}", normalize_windows_process_name(name));

    Command::new("tasklist")
        .args(["/FI", &filter, "/FO", "CSV", "/NH"])
        .output()
        .map(|output| {
            if !output.status.success() {
                return false;
            }

            let text = String::from_utf8_lossy(&output.stdout).to_ascii_lowercase();
            text.contains(&normalize_windows_process_name(name).to_ascii_lowercase())
        })
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn find_pids_by_name_impl(name: &str) -> Result<Vec<u32>> {
    let output = Command::new("pgrep")
        .arg("-x")
        .arg(name)
        .output()
        .context("Failed to run pgrep")?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    parse_pid_lines(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(windows)]
fn find_pids_by_name_impl(name: &str) -> Result<Vec<u32>> {
    let process_name = normalize_windows_process_name(name)
        .trim_end_matches(".exe")
        .to_string();
    let escaped = process_name.replace('\'', "''");
    let script = format!(
        "Get-Process -Name '{}' -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Id",
        escaped
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()
        .context("Failed to run powershell Get-Process")?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    parse_pid_lines(&String::from_utf8_lossy(&output.stdout))
}

fn parse_pid_lines(text: &str) -> Result<Vec<u32>> {
    let mut pids = Vec::new();

    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let pid = line
            .parse::<u32>()
            .with_context(|| format!("Failed to parse process id: {}", line))?;
        pids.push(pid);
    }

    Ok(pids)
}

#[cfg(windows)]
fn normalize_windows_process_name(name: &str) -> String {
    if name.to_ascii_lowercase().ends_with(".exe") {
        name.to_string()
    } else {
        format!("{}.exe", name)
    }
}
