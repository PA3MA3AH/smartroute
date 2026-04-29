use crate::{
    config::{load_config, validate_config, Rule},
    tester::{find_best_node_for_url, test_single_node_for_url},
    util::write_config_toml,
};
use anyhow::{Context, Result};
use std::{
    path::Path,
    process::Command,
    thread,
    time::Duration,
};

pub fn diagnose_site(
    input: &Path,
    output: Option<&Path>,
    domain: &str,
    timeout: u64,
    jobs: usize,
    samples: usize,
    hysteresis_ms: u64,
    force: bool,
) -> Result<()> {
    let domain = normalize_domain(domain);
    let target_url = format!("https://{}", domain);

    println!("Diagnosing domain: {}", domain);
    println!("HTTP probe target: {}", target_url);

    let mut config = load_config(input)?;
    validate_config(&config)?;

    if let Some(existing_owned) = existing_rule_outbound(&config.rules, &domain).map(str::to_string) {
        println!("Existing rule found: {} -> {}", domain, existing_owned);

        let proxy_ok = proxy_check(&domain, timeout)?;

        if proxy_ok && is_sticky_domain(&domain) && !force {
            println!(
                "Sticky domain {} already opens through current SOCKS route. Keeping {} and skipping speed race.",
                domain, existing_owned
            );
            return Ok(());
        }

        if proxy_ok {
            println!("Existing route passes real HTTPS check. Checking if a much better route exists...");
        } else {
            println!("Existing route FAILED real HTTPS check. Hysteresis will be ignored.");
        }

        let Some(best) = find_best_node_for_url(&config, &target_url, timeout, jobs, samples)? else {
            if proxy_ok {
                println!("No better working proxy found. Keeping existing route.");
                return Ok(());
            }

            anyhow::bail!("Existing route failed and no replacement could open {}", target_url);
        };

        println!(
            "Best route for {}: {} | median={} ms jitter={} ms loss={}%, ok={}/{} score={} ms",
            domain,
            best.tag,
            best.median_ms,
            best.jitter_ms,
            best.loss_percent,
            best.ok,
            best.total,
            best.score_ms
        );

        if existing_owned == best.tag {
            if proxy_ok {
                println!("Same route as existing and HTTPS check is OK. No change.");
            } else {
                println!("Best candidate is the same as existing, but current live check failed. Restart may still help.");
            }
            return Ok(());
        }

        if !proxy_ok {
            println!(
                "Switching immediately: current route cannot open {}, new route is {}",
                domain, best.tag
            );
            upsert_domain_rule(&mut config.rules, &domain, &best.tag);
            save_config(input, output, &config, &domain, &best.tag)?;
            return Ok(());
        }

        println!("Checking current route performance against same target...");

        if let Some(current) =
            test_single_node_for_url(&config, &existing_owned, &target_url, timeout, samples)?
        {
            println!(
                "Current route: {} | median={} ms jitter={} ms loss={}%, ok={}/{} score={} ms",
                current.tag,
                current.median_ms,
                current.jitter_ms,
                current.loss_percent,
                current.ok,
                current.total,
                current.score_ms
            );

            if current.loss_percent >= 50 {
                println!(
                    "Current route has hard loss {}%. Switching without hysteresis.",
                    current.loss_percent
                );
                upsert_domain_rule(&mut config.rules, &domain, &best.tag);
                save_config(input, output, &config, &domain, &best.tag)?;
                return Ok(());
            }

            let diff = current.score_ms as i128 - best.score_ms as i128;

            if diff < hysteresis_ms as i128 {
                println!(
                    "Improvement too small by score ({} ms < {} ms). Keeping current route.",
                    diff, hysteresis_ms
                );
                return Ok(());
            }

            println!("Switching route (score better by {} ms)", diff);
        } else {
            println!("Current route test failed. Switching without hysteresis.");
        }

        upsert_domain_rule(&mut config.rules, &domain, &best.tag);
        save_config(input, output, &config, &domain, &best.tag)?;
        return Ok(());
    }

    if !force && direct_check(&domain, timeout)? {
        println!("Direct connection works. No proxy rule needed.");
        return Ok(());
    }

    if force {
        println!("Force mode enabled. Selecting proxy anyway...");
    } else {
        println!("Direct connection failed. Searching best proxy...");
    }

    let Some(best) = find_best_node_for_url(&config, &target_url, timeout, jobs, samples)? else {
        anyhow::bail!("No working proxy nodes found for {}", target_url);
    };

    println!(
        "Best route for {}: {} | median={} ms jitter={} ms loss={}%, ok={}/{} score={} ms",
        domain,
        best.tag,
        best.median_ms,
        best.jitter_ms,
        best.loss_percent,
        best.ok,
        best.total,
        best.score_ms
    );

    upsert_domain_rule(&mut config.rules, &domain, &best.tag);
    save_config(input, output, &config, &domain, &best.tag)?;

    Ok(())
}

pub fn watch_sites(
    input: &Path,
    domains: Vec<String>,
    interval: u64,
    timeout: u64,
    jobs: usize,
    samples: usize,
    hysteresis_ms: u64,
) -> Result<()> {
    if domains.is_empty() {
        anyhow::bail!("No domains provided");
    }

    println!("SmartRoute auto-diagnose started");
    println!("interval: {}s", interval);
    println!("domains: {:?}", domains);
    println!("hysteresis: {} ms", hysteresis_ms);
    println!();

    loop {
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
                println!();
                continue;
            }
        
            let _ = diagnose_site(
                input,
                None,
                domain,
                timeout,
                jobs,
                samples,
                hysteresis_ms,
                false,
            );
        
            println!();
        }

        thread::sleep(Duration::from_secs(interval));
    }
}

fn direct_check(domain: &str, timeout: u64) -> Result<bool> {
    let url = format!("https://{}", domain);

    let output = Command::new("curl")
        .arg("--noproxy")
        .arg("*")
        .arg("-L")
        .arg("-m")
        .arg(timeout.to_string())
        .arg("-sS")
        .arg("-o")
        .arg("/dev/null")
        .arg("-w")
        .arg("%{http_code}")
        .arg(&url)
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("all_proxy")
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY")
        .output()
        .with_context(|| format!("Failed to check direct access to {}", domain))?;

    if !output.status.success() {
        return Ok(false);
    }

    let code_raw = String::from_utf8_lossy(&output.stdout);
    let code: u16 = code_raw.trim().parse().unwrap_or(0);

    println!("Direct HTTP status: {}", code);

    Ok((200..500).contains(&code))
}

fn proxy_check(domain: &str, timeout: u64) -> Result<bool> {
    let url = format!("https://{}", domain);

    let output = Command::new("curl")
        .arg("--noproxy")
        .arg("*")
        .arg("--socks5-hostname")
        .arg("127.0.0.1:1081")
        .arg("--connect-timeout")
        .arg(timeout.to_string())
        .arg("-L")
        .arg("-m")
        .arg(timeout.to_string())
        .arg("-sS")
        .arg("-o")
        .arg("/dev/null")
        .arg("-w")
        .arg("%{http_code}")
        .arg(&url)
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("all_proxy")
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY")
        .output()
        .with_context(|| format!("Failed to check proxy access to {}", domain))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            println!("Proxy HTTPS check failed at curl level, exit={:?}", output.status.code());
        } else {
            println!(
                "Proxy HTTPS check failed at curl level, exit={:?}: {}",
                output.status.code(),
                stderr
            );
        }
        return Ok(false);
    }

    let code_raw = String::from_utf8_lossy(&output.stdout);
    let code: u16 = code_raw.trim().parse().unwrap_or(0);

    println!("Proxy HTTP status: {}", code);

    Ok((200..500).contains(&code))
}

fn save_config(
    input: &Path,
    output: Option<&Path>,
    config: &crate::config::SmartRouteConfig,
    domain: &str,
    outbound: &str,
) -> Result<()> {
    let out_path = output.unwrap_or(input);
    write_config_toml(out_path, config)?;

    println!("Rule saved:");
    println!("  domain_suffix {} -> {}", domain, outbound);
    println!("Saved config: {}", out_path.display());

    Ok(())
}

fn upsert_domain_rule(rules: &mut Vec<Rule>, domain: &str, outbound: &str) {
    for rule in rules.iter_mut() {
        if rule.rule_type == "domain_suffix" && rule.value == domain {
            rule.outbound = outbound.to_string();
            return;
        }
    }

    rules.push(Rule {
        rule_type: "domain_suffix".to_string(),
        value: domain.to_string(),
        outbound: outbound.to_string(),
    });
}

fn normalize_domain(input: &str) -> String {
    let mut s = input.trim().to_string();

    if let Some(rest) = s.strip_prefix("https://") {
        s = rest.to_string();
    }

    if let Some(rest) = s.strip_prefix("http://") {
        s = rest.to_string();
    }

    if let Some(pos) = s.find('/') {
        s.truncate(pos);
    }

    if let Some(rest) = s.strip_prefix("www.") {
        s = rest.to_string();
    }

    s
}

fn existing_rule_outbound<'a>(rules: &'a [Rule], domain: &str) -> Option<&'a str> {
    rules
        .iter()
        .find(|r| r.rule_type == "domain_suffix" && r.value == domain)
        .map(|r| r.outbound.as_str())
}

fn is_sticky_domain(domain: &str) -> bool {
    domain == "chatgpt.com"
        || domain.ends_with(".chatgpt.com")
        || domain == "openai.com"
        || domain.ends_with(".openai.com")
        || domain == "oaistatic.com"
        || domain.ends_with(".oaistatic.com")
        || domain == "oaiusercontent.com"
        || domain.ends_with(".oaiusercontent.com")
}
