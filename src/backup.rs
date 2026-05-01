use anyhow::{Context, Result};
use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub fn backup_config(input: &Path) -> Result<PathBuf> {
    let backup = create_backup_if_exists(input)?.with_context(|| {
        format!(
            "Config does not exist, nothing to backup: {}",
            input.display()
        )
    })?;

    println!("Backup created: {}", backup.display());

    Ok(backup)
}

pub fn create_backup_if_exists(input: &Path) -> Result<Option<PathBuf>> {
    if !input.exists() {
        return Ok(None);
    }

    if !input.is_file() {
        anyhow::bail!("Cannot backup non-file path: {}", input.display());
    }

    let dir = backups_dir()?;
    fs::create_dir_all(&dir).with_context(|| format!("Failed to create {}", dir.display()))?;

    let backup_path = unique_backup_path(input, &dir)?;

    fs::copy(input, &backup_path).with_context(|| {
        format!(
            "Failed to copy backup {} -> {}",
            input.display(),
            backup_path.display()
        )
    })?;

    Ok(Some(backup_path))
}

pub fn list_backups(input: Option<&Path>) -> Result<()> {
    let backups = collect_backups(input)?;

    println!("SmartRoute backups");
    println!("────────────────────────────────────────────────────────");

    if let Some(input) = input {
        println!("Config filter: {}", input.display());
    } else {
        println!("Config filter: all");
    }

    println!("Directory: {}", backups_dir()?.display());
    println!();

    if backups.is_empty() {
        println!("No backups found.");
        return Ok(());
    }

    for backup in backups {
        let size = fs::metadata(&backup).map(|m| m.len()).unwrap_or(0);

        println!("{}  ({} bytes)", backup.display(), size);
    }

    Ok(())
}

pub fn restore_backup(target: &Path, file: Option<&Path>) -> Result<PathBuf> {
    let backup = match file {
        Some(file) => resolve_backup_path(file)?,
        None => latest_backup_for(target)?,
    };

    if !backup.exists() {
        anyhow::bail!("Backup file does not exist: {}", backup.display());
    }

    if target.exists() {
        if let Some(pre_restore) = create_backup_if_exists(target)? {
            println!("Pre-restore backup created: {}", pre_restore.display());
        }
    }

    fs::copy(&backup, target).with_context(|| {
        format!(
            "Failed to restore backup {} -> {}",
            backup.display(),
            target.display()
        )
    })?;

    println!("Restored backup:");
    println!("  from: {}", backup.display());
    println!("  to:   {}", target.display());

    Ok(backup)
}

pub fn backups_dir() -> Result<PathBuf> {
    if let Ok(state_home) = env::var("XDG_STATE_HOME") {
        return Ok(PathBuf::from(state_home).join("smartroute").join("backups"));
    }

    let home = env::var("HOME").context("HOME is not set")?;

    Ok(PathBuf::from(home)
        .join(".local")
        .join("state")
        .join("smartroute")
        .join("backups"))
}

fn latest_backup_for(target: &Path) -> Result<PathBuf> {
    let backups = collect_backups(Some(target))?;

    backups
        .last()
        .cloned()
        .with_context(|| format!("No backups found for {}", target.display()))
}

fn collect_backups(input: Option<&Path>) -> Result<Vec<PathBuf>> {
    let dir = backups_dir()?;

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let prefix = input.map(|path| format!("{}-", backup_stem(path)));

    let mut backups = Vec::new();

    for entry in fs::read_dir(&dir).with_context(|| format!("Failed to read {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }

        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        if let Some(prefix) = &prefix {
            if !name.starts_with(prefix) {
                continue;
            }
        }

        backups.push(path);
    }

    backups.sort();

    Ok(backups)
}

fn resolve_backup_path(file: &Path) -> Result<PathBuf> {
    if file.exists() {
        return Ok(file.to_path_buf());
    }

    let candidate = backups_dir()?.join(file);

    if candidate.exists() {
        return Ok(candidate);
    }

    Ok(file.to_path_buf())
}

fn unique_backup_path(input: &Path, dir: &Path) -> Result<PathBuf> {
    let stem = backup_stem(input);
    let ts = timestamp_millis()?;

    for idx in 0..1000u32 {
        let name = if idx == 0 {
            format!("{}-{}.toml", stem, ts)
        } else {
            format!("{}-{}-{}.toml", stem, ts, idx)
        };

        let path = dir.join(name);

        if !path.exists() {
            return Ok(path);
        }
    }

    anyhow::bail!("Failed to create unique backup filename");
}

fn backup_stem(input: &Path) -> String {
    let raw = input
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("config");

    let mut out = String::new();
    let mut last_dash = false;

    for ch in raw.chars() {
        let mapped = if ch.is_ascii_alphanumeric() {
            Some(ch.to_ascii_lowercase())
        } else if ch == '-' || ch == '_' || ch.is_ascii_whitespace() {
            Some('-')
        } else {
            None
        };

        if let Some(ch) = mapped {
            if ch == '-' {
                if !last_dash && !out.is_empty() {
                    out.push('-');
                    last_dash = true;
                }
            } else {
                out.push(ch);
                last_dash = false;
            }
        }
    }

    while out.ends_with('-') {
        out.pop();
    }

    if out.is_empty() {
        "config".to_string()
    } else {
        out
    }
}

fn timestamp_millis() -> Result<u128> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("System clock is before UNIX_EPOCH")?
        .as_millis())
}
