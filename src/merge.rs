use crate::config::{load_config, save_config};
use anyhow::{Context, Result};
use std::path::Path;

pub fn merge_nodes(base_path: &Path, nodes_path: &Path, output: Option<&Path>) -> Result<()> {
    let mut base = load_config(base_path)
        .with_context(|| format!("Failed to read base config: {}", base_path.display()))?;

    let fresh = load_config(nodes_path)
        .with_context(|| format!("Failed to read nodes config: {}", nodes_path.display()))?;

    let old_count = base.nodes.len();
    let new_count = fresh.nodes.len();

    base.nodes = fresh.nodes;

    let base_url_empty = base
        .subscription
        .url
        .as_deref()
        .map(str::is_empty)
        .unwrap_or(true);

    let fresh_url_non_empty = fresh
        .subscription
        .url
        .as_deref()
        .map(|url| !url.is_empty())
        .unwrap_or(false);

    if base_url_empty && fresh_url_non_empty {
        base.subscription.url = fresh.subscription.url;
    }

    if base.subscription.auto_refresh == 0 && fresh.subscription.auto_refresh > 0 {
        base.subscription.auto_refresh = fresh.subscription.auto_refresh;
    }

    let has_subscription_url = base
        .subscription
        .url
        .as_deref()
        .map(|url| !url.is_empty())
        .unwrap_or(false);

    let output = output.unwrap_or(base_path);

    save_config(output, &base)
        .with_context(|| format!("Failed to save merged config: {}", output.display()))?;

    println!("Merged nodes:");
    println!("  base config: {}", base_path.display());
    println!("  nodes from:  {}", nodes_path.display());
    println!("  output:      {}", output.display());
    println!("  old nodes:   {}", old_count);
    println!("  new nodes:   {}", new_count);

    if has_subscription_url {
        println!("  subscription URL: kept/set");
    }

    Ok(())
}
