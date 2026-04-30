use crate::{
    config::{load_config, validate_config, Node, SmartRouteConfig},
    singbox::generate_singbox_config,
    util::{sanitize_tag, write_config_toml},
};
use anyhow::{Context, Result};
use std::{
    env, fs,
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone)]
struct TestResult {
    tag: String,
    median_ms: Option<u128>,
    jitter_ms: Option<u128>,
    score_ms: Option<u128>,
    ok: usize,
    total: usize,
    loss_percent: u128,
    last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NodeScore {
    pub tag: String,
    pub median_ms: u128,
    pub jitter_ms: u128,
    pub score_ms: u128,
    pub ok: usize,
    pub total: usize,
    pub loss_percent: u128,
}

pub fn find_best_node(
    config: &SmartRouteConfig,
    timeout: u64,
    jobs: usize,
    samples: usize,
) -> Result<Option<NodeScore>> {
    find_best_node_for_url(config, "https://ifconfig.me", timeout, jobs, samples)
}

pub fn find_best_node_for_url(
    config: &SmartRouteConfig,
    url: &str,
    timeout: u64,
    jobs: usize,
    samples: usize,
) -> Result<Option<NodeScore>> {
    let results = test_all_nodes_parallel(config, url, timeout, jobs.max(1), samples.max(1))?;
    let mut best: Option<NodeScore> = None;

    for result in results {
        let (Some(median), Some(jitter), Some(score)) =
            (result.median_ms, result.jitter_ms, result.score_ms)
        else {
            continue;
        };

        if result.ok == 0 || result.loss_percent >= 50 {
            continue;
        }

        let candidate = NodeScore {
            tag: result.tag,
            median_ms: median,
            jitter_ms: jitter,
            score_ms: score,
            ok: result.ok,
            total: result.total,
            loss_percent: result.loss_percent,
        };

        if best
            .as_ref()
            .map(|b| candidate.score_ms < b.score_ms)
            .unwrap_or(true)
        {
            best = Some(candidate);
        }
    }

    Ok(best)
}

pub fn test_single_node(
    config: &SmartRouteConfig,
    node_tag: &str,
    timeout: u64,
    samples: usize,
) -> Result<Option<NodeScore>> {
    test_single_node_for_url(config, node_tag, "https://ifconfig.me", timeout, samples)
}

pub fn test_single_node_for_url(
    config: &SmartRouteConfig,
    node_tag: &str,
    url: &str,
    timeout: u64,
    samples: usize,
) -> Result<Option<NodeScore>> {
    let Some(node) = config.nodes.iter().find(|n| n.tag == node_tag) else {
        return Ok(None);
    };

    let samples = samples.max(1);
    let mut times = Vec::new();

    for _ in 0..samples {
        match test_single(config, node, url, timeout)? {
            ProbeResult::Ok(ms) => times.push(ms),
            ProbeResult::Fail(_) => {}
        }
    }

    if times.is_empty() {
        return Ok(None);
    }

    times.sort_unstable();

    let median = times[times.len() / 2];
    let min = *times.first().unwrap();
    let max = *times.last().unwrap();
    let jitter = max - min;
    let ok = times.len();
    let loss_percent = (((samples - ok) as u128) * 100) / (samples as u128);
    let score_ms = score(median, jitter, loss_percent);

    Ok(Some(NodeScore {
        tag: node_tag.to_string(),
        median_ms: median,
        jitter_ms: jitter,
        score_ms,
        ok,
        total: samples,
        loss_percent,
    }))
}

pub fn test_nodes(input: &Path, timeout: u64, jobs: usize, samples: usize) -> Result<()> {
    warn_if_proxy_environment_exists();

    let config = load_config(input)?;
    validate_config(&config)?;

    let jobs = jobs.max(1);
    let samples = samples.max(1);
    let target = "https://ifconfig.me";

    println!("Testing nodes through real temporary SOCKS instances...");
    println!("target: {}", target);
    println!("timeout: {}s, jobs: {}, samples: {}", timeout, jobs, samples);
    println!("score = median + jitter*2 + loss%*5");
    println!("IMPORTANT: each candidate is tested with all rules cleared, so old domain rules cannot fake the result.\n");

    let results = test_all_nodes_parallel(&config, target, timeout, jobs, samples)?;
    print_results(results);

    Ok(())
}

pub fn auto_select_fastest(
    input: &Path,
    output: Option<&Path>,
    timeout: u64,
    jobs: usize,
    samples: usize,
) -> Result<()> {
    warn_if_proxy_environment_exists();

    let config = load_config(input)?;
    validate_config(&config)?;

    let jobs = jobs.max(1);
    let samples = samples.max(1);
    let target = "https://ifconfig.me";

    println!("Finding best stable node through real temporary SOCKS instances...");
    println!("target: {}", target);
    println!("timeout: {}s, jobs: {}, samples: {}", timeout, jobs, samples);
    println!("score = median + jitter*2 + loss%*5\n");

    let results = test_all_nodes_parallel(&config, target, timeout, jobs, samples)?;
    let mut best: Option<(String, u128, u128, u128, u128)> = None;

    for result in &results {
        match (result.median_ms, result.jitter_ms, result.score_ms) {
            (Some(median), Some(jitter), Some(score)) => {
                println!(
                    "{:<30} ✅ median={} ms jitter={} ms loss={}%, ok={}/{} score={} ms [{}]",
                    result.tag,
                    median,
                    jitter,
                    result.loss_percent,
                    result.ok,
                    result.total,
                    score,
                    stability_label(median, jitter, result.loss_percent)
                );

                if result.loss_percent < 50 && (best.is_none() || score < best.as_ref().unwrap().1) {
                    best = Some((
                        result.tag.clone(),
                        score,
                        median,
                        jitter,
                        result.loss_percent,
                    ));
                }
            }
            _ => {
                println!(
                    "{:<30} ❌ failed, loss=100%, ok=0/{}{}",
                    result.tag,
                    result.total,
                    result
                        .last_error
                        .as_deref()
                        .map(|e| format!(" ({})", compact_error(e)))
                        .unwrap_or_default()
                );
            }
        }
    }

    let Some((best_tag, best_score, best_median, best_jitter, best_loss)) = best else {
        println!("\nNo working nodes found");
        return Ok(());
    };

    println!(
        "\nBest node: {} | median={} ms jitter={} ms loss={}% score={} ms",
        best_tag, best_median, best_jitter, best_loss, best_score
    );

    let mut new_config = config;
    new_config.general.final_outbound = best_tag;

    let out_path = output.unwrap_or(input);
    write_config_toml(out_path, &new_config)?;

    println!("Saved config: {}", out_path.display());

    Ok(())
}

fn test_all_nodes_parallel(
    config: &SmartRouteConfig,
    url: &str,
    timeout: u64,
    jobs: usize,
    samples: usize,
) -> Result<Vec<TestResult>> {
    let nodes = config.nodes.clone();
    let mut results = Vec::new();

    for chunk in nodes.chunks(jobs) {
        let mut handles = Vec::new();

        for node in chunk {
            let config_clone = config.clone();
            let node_clone = node.clone();
            let url = url.to_string();

            let handle = thread::spawn(move || {
                test_node_samples(config_clone, node_clone, &url, timeout, samples)
            });

            handles.push(handle);
        }

        for handle in handles {
            match handle.join() {
                Ok(Ok(result)) => results.push(result),
                Ok(Err(err)) => results.push(TestResult {
                    tag: format!("internal-error"),
                    median_ms: None,
                    jitter_ms: None,
                    score_ms: None,
                    ok: 0,
                    total: samples,
                    loss_percent: 100,
                    last_error: Some(err.to_string()),
                }),
                Err(_) => results.push(TestResult {
                    tag: "thread-panic".to_string(),
                    median_ms: None,
                    jitter_ms: None,
                    score_ms: None,
                    ok: 0,
                    total: samples,
                    loss_percent: 100,
                    last_error: Some("worker thread panicked".to_string()),
                }),
            }
        }
    }

    Ok(results)
}

fn test_node_samples(
    config: SmartRouteConfig,
    node: Node,
    url: &str,
    timeout: u64,
    samples: usize,
) -> Result<TestResult> {
    let samples = samples.max(1);
    let mut times = Vec::new();
    let mut last_error = None;

    for _ in 0..samples {
        match test_single(&config, &node, url, timeout)? {
            ProbeResult::Ok(ms) => times.push(ms),
            ProbeResult::Fail(err) => last_error = Some(err),
        }
    }

    let ok = times.len();
    let loss_percent = (((samples - ok) as u128) * 100) / (samples as u128);

    if times.is_empty() {
        return Ok(TestResult {
            tag: node.tag,
            median_ms: None,
            jitter_ms: None,
            score_ms: None,
            ok,
            total: samples,
            loss_percent,
            last_error,
        });
    }

    times.sort_unstable();

    let median = times[times.len() / 2];
    let min = *times.first().unwrap();
    let max = *times.last().unwrap();
    let jitter = max - min;
    let score_ms = score(median, jitter, loss_percent);

    Ok(TestResult {
        tag: node.tag,
        median_ms: Some(median),
        jitter_ms: Some(jitter),
        score_ms: Some(score_ms),
        ok,
        total: samples,
        loss_percent,
        last_error,
    })
}

fn is_ai_access_url(url: &str) -> bool {
    let u = url.to_ascii_lowercase();
    AI_ACCESS_DOMAINS.iter().any(|domain| {
        u.contains(&format!("://{}", domain)) || u.contains(&format!(".{}", domain))
    })
}

const AI_ACCESS_DOMAINS: &[&str] = &[
    "chatgpt.com", "openai.com", "claude.com", "gemini.google.com",
    "aistudio.google.com", "copilot.microsoft.com", "bing.com",
    "perplexity.ai", "venice.ai", "poe.com", "grok.com", "x.ai",
    "meta.ai", "mistral.ai", "chat.mistral.ai", "you.com", "phind.com",
    "huggingface.co",
];

fn ai_geo_block_reason(effective_url: &str, body: &str) -> Option<String> {
    let url = effective_url.to_ascii_lowercase();
    let text = body.to_ascii_lowercase();

    let url_markers = [
        "app-unavailable-in-region", "unsupported-country", "unsupported_region",
        "unavailable-in-region", "region-not-supported",
    ];
    for marker in url_markers {
        if url.contains(marker) {
            return Some(format!("geo-block redirect/url marker: {}", marker));
        }
    }

    let body_markers = [
        "error 1009", "access denied",
        "has banned the country or region your ip address is in",
        "not available in your country", "not available in your region",
        "isn't available in your country", "isnt available in your country",
        "is not available in your country", "is not available in your region",
        "unavailable in your region", "unsupported country", "unsupported region",
        "your country is not supported", "your region is not supported",
        "gemini isn't supported in your country", "gemini is not supported in your country",
        "gemini пока не поддерживается в вашей стране", "недоступно в вашем регионе",
        "недоступен в вашем регионе", "недоступно в вашей стране",
        "пока не поддерживается в вашей стране",
    ];
    for marker in body_markers {
        if text.contains(marker) {
            return Some(format!("geo-block body marker: {}", marker));
        }
    }
    None
}

fn read_file_limited(path: &str, limit: usize) -> String {
    match fs::read(path) {
        Ok(mut data) => {
            if data.len() > limit { data.truncate(limit); }
            String::from_utf8_lossy(&data).to_string()
        }
        Err(_) => String::new(),
    }
}

#[derive(Debug)]
enum ProbeResult {
    Ok(u128),
    Fail(String),
}

fn test_single(
    base_config: &SmartRouteConfig,
    node: &Node,
    url: &str,
    timeout: u64,
) -> Result<ProbeResult> {
    let mut temp_config = base_config.clone();
    let port = pick_test_port(&node.tag);

    temp_config.general.mode = "socks".to_string();
    temp_config.general.listen = "127.0.0.1".to_string();
    temp_config.general.listen_port = port;
    temp_config.general.final_outbound = node.tag.clone();

    // Critical fix:
    // Do NOT keep imported.toml domain rules inside the temporary tester config.
    // Otherwise testing candidate A for chatgpt.com may silently follow an old rule
    // like chatgpt.com -> candidate B, producing fake "working" results.
    temp_config.rules.clear();

    let singbox_config = generate_singbox_config(&temp_config)?;

    let safe_tag = sanitize_tag(&node.tag);
    let pid = std::process::id();
    let config_path = format!("/tmp/smartroute-test-{}-{}-{}.json", pid, port, safe_tag);

    let pretty = serde_json::to_string_pretty(&singbox_config)?;
    fs::write(&config_path, pretty)?;

    let mut child = start_temp_singbox(&config_path)?;
    thread::sleep(Duration::from_millis(700));

    if child.try_wait()?.is_some() {
        let _ = fs::remove_file(&config_path);
        return Ok(ProbeResult::Fail("temporary sing-box exited early".to_string()));
    }

    let body_path = format!("/tmp/smartroute-test-body-{}-{}-{}.html", pid, port, safe_tag);
    let ai_access_probe = is_ai_access_url(url);
    let start = Instant::now();

    let output = Command::new("curl")
        .arg("--noproxy")
        .arg("*")
        .arg("--socks5-hostname")
        .arg(format!("127.0.0.1:{}", port))
        .arg("--connect-timeout")
        .arg(timeout.to_string())
        .arg("-m")
        .arg(timeout.to_string())
        .arg("-sS")
        .arg("-L")
        .arg("-A")
        .arg("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 Chrome/123 Safari/537.36")
        .arg("-o")
        .arg(&body_path)
        .arg("-w")
        .arg("%{http_code} %{url_effective}")
        .arg(url)
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("all_proxy")
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY")
        .output();

    let duration = start.elapsed();

    stop_child(&mut child);
    let _ = fs::remove_file(&config_path);

    let output = match output {
        Ok(output) => output,
        Err(err) => {
            let _ = fs::remove_file(&body_path);
            return Ok(ProbeResult::Fail(format!("failed to run curl: {}", err)));
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let mut parts = stdout.splitn(2, ' ');
    let status_code: u16 = parts.next().unwrap_or("0").trim().parse().unwrap_or(0);
    let effective_url = parts.next().unwrap_or(url).trim().to_string();
    let body = if ai_access_probe { read_file_limited(&body_path, 512 * 1024) } else { String::new() };
    let _ = fs::remove_file(&body_path);

    if output.status.success() && (200..500).contains(&status_code) {
        if ai_access_probe {
            if let Some(reason) = ai_geo_block_reason(&effective_url, &body) {
                return Ok(ProbeResult::Fail(format!(
                    "AI access blocked through this IP: {}; http_code={}, url={}",
                    reason, status_code, effective_url
                )));
            }
        }

        return Ok(ProbeResult::Ok(duration.as_millis()));
    }

    let err = if stderr.is_empty() {
        format!("curl failed, http_code={}, url={}, exit={:?}", status_code, effective_url, output.status.code())
    } else {
        format!(
            "curl failed, http_code={}, url={}, exit={:?}, stderr={}",
            status_code,
            effective_url,
            output.status.code(),
            stderr
        )
    };

    Ok(ProbeResult::Fail(err))
}

fn start_temp_singbox(config_path: &str) -> Result<Child> {
    let child = Command::new("sing-box")
        .arg("run")
        .arg("-c")
        .arg(config_path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Failed to start temporary sing-box")?;

    Ok(child)
}

fn stop_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn pick_test_port(tag: &str) -> u16 {
    let mut hash: u32 = 0;

    for b in tag.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(b as u32);
    }

    12000 + (hash % 20000) as u16
}

fn score(median: u128, jitter: u128, loss_percent: u128) -> u128 {
    median + jitter.saturating_mul(2) + loss_percent.saturating_mul(5)
}

fn print_results(results: Vec<TestResult>) {
    for result in results {
        match (result.median_ms, result.jitter_ms, result.score_ms) {
            (Some(median), Some(jitter), Some(score)) => {
                println!(
                    "{:<30} ✅ median={} ms jitter={} ms loss={}%, ok={}/{} score={} ms [{}]",
                    result.tag,
                    median,
                    jitter,
                    result.loss_percent,
                    result.ok,
                    result.total,
                    score,
                    stability_label(median, jitter, result.loss_percent)
                );
            }
            _ => {
                println!(
                    "{:<30} ❌ failed, loss=100%, ok=0/{}{}",
                    result.tag,
                    result.total,
                    result
                        .last_error
                        .as_deref()
                        .map(|e| format!(" ({})", compact_error(e)))
                        .unwrap_or_default()
                );
            }
        }
    }
}

fn compact_error(error: &str) -> String {
    let mut s = error.replace('\n', " ");

    while s.contains("  ") {
        s = s.replace("  ", " ");
    }

    if s.len() > 140 {
        s.truncate(140);
        s.push_str("...");
    }

    s
}

fn warn_if_proxy_environment_exists() {
    let vars = [
        "http_proxy",
        "https_proxy",
        "all_proxy",
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
    ];

    let mut found = false;

    for v in vars {
        if let Ok(val) = env::var(v) {
            if !val.trim().is_empty() {
                if !found {
                    println!("⚠ proxy environment detected:");
                    found = true;
                }

                println!("  {}={}", v, val);
            }
        }
    }

    if found {
        println!();
        println!("SmartRoute will try to bypass proxy env using curl --noproxy \"*\".");
        println!("Note: this does not bypass system-wide TUN/VPN routing.");
        println!();
    }
}

fn stability_label(median: u128, jitter: u128, loss_percent: u128) -> &'static str {
    if loss_percent >= 50 {
        "bad-loss"
    } else if loss_percent > 0 {
        "lossy"
    } else if jitter <= 15 && median <= 300 {
        "excellent"
    } else if jitter <= 50 && median <= 700 {
        "good"
    } else if jitter <= 120 {
        "unstable"
    } else {
        "bad"
    }
}
