use crate::config::{load_config, save_config};
use anyhow::{Context, Result};
use std::path::Path;

const ALLOWED_FINGERPRINTS: &[&str] = &[
    "chrome",
    "firefox",
    "edge",
    "safari",
    "360",
    "qq",
    "ios",
    "android",
    "random",
    "randomized",
];

pub fn list_masks(input: &Path) -> Result<()> {
    let config = load_config(input)?;

    println!("Traffic camouflage / Reality-uTLS nodes:");
    println!("────────────────────────────────────────────────────────");

    let mut found = false;

    for node in &config.nodes {
        if node.node_type != "vless" {
            continue;
        }

        found = true;

        println!("tag: {}", node.tag);
        println!("  server: {}:{}", node.server, node.port);
        println!("  security: {}", node.security.as_deref().unwrap_or("none"));
        println!(
            "  server_name: {}",
            node.server_name.as_deref().unwrap_or("-")
        );
        println!(
            "  utls_fingerprint: {}",
            node.utls_fingerprint.as_deref().unwrap_or("chrome")
        );

        if node.security.as_deref() == Some("reality") {
            println!(
                "  reality_public_key: {}",
                node.reality_public_key.as_deref().unwrap_or("-")
            );
            println!(
                "  reality_short_id: {}",
                node.reality_short_id.as_deref().unwrap_or("-")
            );
        }

        println!();
    }

    if !found {
        println!("No VLESS nodes found.");
    }

    Ok(())
}

pub fn set_mask(
    input: &Path,
    tag: &str,
    server_name: Option<&str>,
    fingerprint: Option<&str>,
    output: Option<&Path>,
) -> Result<()> {
    let mut config = load_config(input)?;

    if let Some(fp) = fingerprint {
        validate_fingerprint(fp)?;
    }

    let node = config
        .nodes
        .iter_mut()
        .find(|node| node.tag == tag)
        .with_context(|| format!("Node not found: {}", tag))?;

    if node.node_type != "vless" {
        anyhow::bail!("Mask settings are supported only for VLESS nodes");
    }

    if let Some(name) = server_name {
        let name = name.trim();

        if name.is_empty() {
            anyhow::bail!("server_name cannot be empty");
        }

        node.server_name = Some(name.to_string());
    }

    if let Some(fp) = fingerprint {
        node.utls_fingerprint = Some(fp.to_string());
    }

    let save_path = output.unwrap_or(input);
    save_config(save_path, &config)?;

    println!("Mask updated for node: {}", tag);

    if let Some(name) = server_name {
        println!("  server_name = {}", name);
    }

    if let Some(fp) = fingerprint {
        println!("  utls_fingerprint = {}", fp);
    }

    println!("Saved config: {}", save_path.display());
    println!();
    println!(
        "Important: changing server_name may break Reality node if server does not allow this SNI."
    );

    Ok(())
}

fn validate_fingerprint(fp: &str) -> Result<()> {
    if ALLOWED_FINGERPRINTS.contains(&fp) {
        Ok(())
    } else {
        anyhow::bail!(
            "Unsupported fingerprint: {}. Allowed: {}",
            fp,
            ALLOWED_FINGERPRINTS.join(", ")
        );
    }
}
