use crate::{
    diagnosis::diagnose_site,
    runtime::{start_smartroute, stop_smartroute},
};
use anyhow::{Context, Result};
use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant, SystemTime},
};

pub fn run_daemon(
    input: &Path,
    interval: u64,
    domains: Vec<String>,
    diagnose_interval: u64,
    timeout: u64,
    jobs: usize,
    samples: usize,
    hysteresis_ms: u64,
    force: bool,
) -> Result<()> {
    println!("SmartRoute daemon started");
    println!("Config: {}", input.display());
    println!("Restart check interval: {}s", interval);

    if domains.is_empty() {
        println!("Auto-diagnose: disabled");
    } else {
        println!("Auto-diagnose interval: {}s", diagnose_interval);
        println!("Domains: {:?}", domains);
        println!("Hysteresis: {} ms", hysteresis_ms);
        println!("Force diagnose: {}", force);
    }

    println!("Press Ctrl+C to stop");
    println!();

    let mut last_modified = get_modified_time(input)?;
    let mut last_diagnose = Instant::now()
        .checked_sub(Duration::from_secs(diagnose_interval))
        .unwrap_or_else(Instant::now);

    restart_smartroute(input)?;

    loop {
        thread::sleep(Duration::from_secs(interval));

        let current_modified = match get_modified_time(input) {
            Ok(time) => time,
            Err(err) => {
                eprintln!("Failed to read config metadata: {}", err);
                continue;
            }
        };

        if current_modified != last_modified {
            println!("Config changed, restarting SmartRoute...");
            last_modified = current_modified;

            if let Err(err) = restart_smartroute(input) {
                eprintln!("Restart failed: {}", err);
            }
        }

        if !domains.is_empty() && last_diagnose.elapsed() >= Duration::from_secs(diagnose_interval)
        {
            println!("Running auto-diagnose...");

            let sticky_domains = [
                "chatgpt.com",
                "openai.com",
                "oaistatic.com",
                "oaiusercontent.com",
                "auth.openai.com",
                "chat.openai.com",
                "cdn.oaistatic.com",
            ];
            
            for domain in &domains {
                if sticky_domains.iter().any(|d| domain.ends_with(d)) {
                    println!("Skipping auto-diagnose for sticky domain: {}", domain);
                    continue;
                }
            
                if let Err(err) = diagnose_site(
                    input,
                    None,
                    domain,
                    timeout,
                    jobs,
                    samples,
                    hysteresis_ms,
                    force,
                ) {
                    eprintln!("Diagnose failed for {}: {}", domain, err);
                }
            }

            last_diagnose = Instant::now();

            let new_modified = get_modified_time(input)?;
            if new_modified != last_modified {
                println!("Rules changed by diagnose, restarting SmartRoute...");
                last_modified = new_modified;

                if let Err(err) = restart_smartroute(input) {
                    eprintln!("Restart failed: {}", err);
                }
            }
        }
    }
}

fn restart_smartroute(input: &Path) -> Result<()> {
    let _ = stop_smartroute();

    start_smartroute(input)
        .with_context(|| format!("Failed to start SmartRoute with {}", input.display()))?;

    Ok(())
}

fn get_modified_time(path: &Path) -> Result<SystemTime> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("Failed to stat {}", path.display()))?;

    metadata
        .modified()
        .with_context(|| format!("Failed to read modified time for {}", path.display()))
}
