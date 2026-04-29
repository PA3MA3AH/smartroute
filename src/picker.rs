use crate::{
    config::{load_config, validate_config},
    util::write_config_toml,
};
use anyhow::Result;
use std::{
    io::{self, Write},
    path::Path,
};

pub fn pick_node(input: &Path, output: Option<&Path>) -> Result<()> {
    let mut config = load_config(input)?;
    validate_config(&config)?;

    if config.nodes.is_empty() {
        println!("No nodes available");
        return Ok(());
    }

    println!("Select node:\n");

    for (i, node) in config.nodes.iter().enumerate() {
        println!(
            "[{}] {} ({}:{} {})",
            i,
            node.tag,
            node.server,
            node.port,
            node.node_type
        );
    }

    print!("\nEnter number: ");
    io::stdout().flush().ok();

    let mut input_line = String::new();
    io::stdin().read_line(&mut input_line)?;

    let index: usize = input_line.trim().parse().unwrap_or(0);

    if index >= config.nodes.len() {
        println!("Invalid selection");
        return Ok(());
    }

    let selected = config.nodes[index].tag.clone();

    println!("\nSelected node: {}", selected);

    config.general.final_outbound = selected;

    let out_path = output.unwrap_or(input);

    write_config_toml(out_path, &config)?;

    println!("Saved config: {}", out_path.display());

    Ok(())
}
