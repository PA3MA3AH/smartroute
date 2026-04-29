use crate::{
    config::{load_config, validate_config, Rule},
    daemon::run_daemon,
    diagnosis::{diagnose_site, watch_sites},
    picker::pick_node,
    runtime::{start_smartroute, status_smartroute, stop_smartroute},
    singbox::generate_singbox_config,
    subscription::import_url,
    tester::{auto_select_fastest, test_nodes},
    util::write_config_toml,
};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
};

#[derive(Parser)]
#[command(name = "smartroute")]
#[command(about = "Smart per-app/per-domain proxy router")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Daemon {
        input: PathBuf,

        #[arg(short, long, default_value_t = 2)]
        interval: u64,

        #[arg(short, long)]
        domain: Vec<String>,

        #[arg(long, default_value_t = 300)]
        diagnose_interval: u64,

        #[arg(long, default_value_t = 8)]
        timeout: u64,

        #[arg(long, default_value_t = 12)]
        jobs: usize,

        #[arg(long, default_value_t = 3)]
        samples: usize,

        #[arg(long, default_value_t = 50)]
        hysteresis: u64,

        #[arg(long, default_value_t = false)]
        force: bool,
    },

    ImportUrl {
        url: String,

        #[arg(short, long)]
        output: PathBuf,
    },

    Generate {
        input: PathBuf,

        #[arg(short, long)]
        output: PathBuf,
    },

    Validate {
        input: PathBuf,
    },

    Nodes {
        input: PathBuf,
    },

    Rule {
        #[command(subcommand)]
        command: RuleCommand,
    },

    Pick {
        input: PathBuf,

        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    Test {
        input: PathBuf,

        #[arg(short, long, default_value_t = 8)]
        timeout: u64,

        #[arg(short, long, default_value_t = 8)]
        jobs: usize,

        #[arg(short, long, default_value_t = 3)]
        samples: usize,
    },

    Auto {
        input: PathBuf,

        #[arg(short, long)]
        output: Option<PathBuf>,

        #[arg(short, long, default_value_t = 8)]
        timeout: u64,

        #[arg(short, long, default_value_t = 8)]
        jobs: usize,

        #[arg(short, long, default_value_t = 3)]
        samples: usize,
    },

    Diagnose {
        input: PathBuf,
        domain: String,

        #[arg(short, long)]
        output: Option<PathBuf>,

        #[arg(short, long, default_value_t = 8)]
        timeout: u64,

        #[arg(short, long, default_value_t = 12)]
        jobs: usize,

        #[arg(short, long, default_value_t = 3)]
        samples: usize,

        #[arg(long, default_value_t = 50)]
        hysteresis: u64,

        #[arg(long, default_value_t = false)]
        force: bool,
    },

    Watch {
        input: PathBuf,

        #[arg(short, long)]
        domain: Vec<String>,

        #[arg(short, long, default_value_t = 300)]
        interval: u64,

        #[arg(short, long, default_value_t = 8)]
        timeout: u64,

        #[arg(short, long, default_value_t = 12)]
        jobs: usize,

        #[arg(short, long, default_value_t = 3)]
        samples: usize,

        #[arg(long, default_value_t = 50)]
        hysteresis: u64,
    },

    Ui {
        #[arg(default_value = "imported.toml")]
        input: PathBuf,
    },

    Start {
        input: PathBuf,
    },

    Stop,

    Status,
}

#[derive(Subcommand)]
enum RuleCommand {
    List {
        input: PathBuf,
    },

    Add {
        input: PathBuf,
        rule_type: String,
        value: String,
        outbound: String,

        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    Remove {
        input: PathBuf,
        index: usize,

        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Daemon {
            input,
            interval,
            domain,
            diagnose_interval,
            timeout,
            jobs,
            samples,
            hysteresis,
            force,
        } => {
            run_daemon(
                &input,
                interval,
                domain,
                diagnose_interval,
                timeout,
                jobs,
                samples,
                hysteresis,
                force,
            )?;
        }

        Commands::ImportUrl { url, output } => {
            import_url(&url, &output)?;
        }

        Commands::Generate { input, output } => {
            let config = load_config(&input)?;
            validate_config(&config)?;

            let singbox_config = generate_singbox_config(&config)?;
            let pretty = serde_json::to_string_pretty(&singbox_config)
                .context("Failed to serialize sing-box config")?;

            fs::write(&output, pretty)
                .with_context(|| format!("Failed to write output: {}", output.display()))?;

            println!("Generated sing-box config: {}", output.display());
        }

        Commands::Validate { input } => {
            let config = load_config(&input)?;
            validate_config(&config)?;
            println!("OK: config is valid");
        }

        Commands::Nodes { input } => {
            let config = load_config(&input)?;
            validate_config(&config)?;

            let mut stdout = io::stdout();

            for node in config.nodes {
                let line = format!(
                    "{} | {}://{}:{}\n",
                    node.tag, node.node_type, node.server, node.port
                );

                if stdout.write_all(line.as_bytes()).is_err() {
                    break;
                }
            }
        }

        Commands::Rule { command } => {
            handle_rule_command(command)?;
        }

        Commands::Pick { input, output } => {
            pick_node(&input, output.as_deref())?;
        }

        Commands::Test {
            input,
            timeout,
            jobs,
            samples,
        } => {
            test_nodes(&input, timeout, jobs, samples)?;
        }

        Commands::Auto {
            input,
            output,
            timeout,
            jobs,
            samples,
        } => {
            auto_select_fastest(&input, output.as_deref(), timeout, jobs, samples)?;
        }

        Commands::Diagnose {
            input,
            domain,
            output,
            timeout,
            jobs,
            samples,
            hysteresis,
            force,
        } => {
            diagnose_site(
                &input,
                output.as_deref(),
                &domain,
                timeout,
                jobs,
                samples,
                hysteresis,
                force,
            )?;
        }

        Commands::Watch {
            input,
            domain,
            interval,
            timeout,
            jobs,
            samples,
            hysteresis,
        } => {
            watch_sites(&input, domain, interval, timeout, jobs, samples, hysteresis)?;
        }

        Commands::Ui { input } => {
            run_ui(input)?;
        }

        Commands::Start { input } => {
            start_smartroute(&input)?;
        }

        Commands::Stop => {
            stop_smartroute()?;
        }

        Commands::Status => {
            status_smartroute()?;
        }
    }

    Ok(())
}

fn run_ui(input: PathBuf) -> Result<()> {
    let mut input = input;

    loop {
        println!();
        println!("=== SmartRoute UI ===");
        println!("Config: {}", input.display());
        println!("1) Start daemon (safe: no ChatGPT auto)");
        println!("2) Start daemon (full auto)");
        println!("3) Stop SmartRoute");
        println!("4) Status");
        println!("5) Diagnose chatgpt.com now");
        println!("6) Diagnose custom domain now");
        println!("7) Add/refresh ChatGPT/OpenAI rules to one outbound");
        println!("8) List rules");
        println!("9) Change config path");
        println!("0) Exit");
        print!("> ");
        io::stdout().flush()?;

        let choice = read_line_trimmed()?;

        match choice.as_str() {
            "1" => {
                let domains = vec![
                    "chatgpt.com".to_string(),
                    "discord.com".to_string(),
                    "youtube.com".to_string(),
                ];
            
                println!("Starting daemon preset. Ctrl+C stops it.");
                run_daemon(&input, 2, domains, 300, 8, 12, 3, 25, false)?;
            }
            "2" => {
                let domains = vec![
                    "chatgpt.com".to_string(),
                    "discord.com".to_string(),
                    "youtube.com".to_string(),
                ];
            
                println!("Starting FULL daemon (no protection)");
                run_daemon(&input, 2, domains, 300, 8, 12, 3, 25, true)?;
            }
            "3" => {
                stop_smartroute()?;
            }
            "4" => {
                status_smartroute()?;
            }
            "5" => {
                diagnose_site(&input, None, "chatgpt.com", 8, 12, 3, 25, false)?;
            }
            "6" => {
                print!("Domain: ");
                io::stdout().flush()?;
                let domain = read_line_trimmed()?;
                if !domain.is_empty() {
                    diagnose_site(&input, None, &domain, 8, 12, 3, 25, false)?;
                }
            }
            "7" => {
                print!("Outbound tag for ChatGPT/OpenAI: ");
                io::stdout().flush()?;
                let outbound = read_line_trimmed()?;
                if outbound.is_empty() {
                    println!("Cancelled: empty outbound");
                } else {
                    add_chatgpt_rules(&input, &outbound)?;
                }
            }
            "8" => {
                handle_rule_command(RuleCommand::List { input: input.clone() })?;
            }
            "9" => {
                print!("New config path: ");
                io::stdout().flush()?;
                let new_path = read_line_trimmed()?;
                if !new_path.is_empty() {
                    input = PathBuf::from(new_path);
                }
            }
            "0" | "q" | "quit" | "exit" => return Ok(()),
            _ => println!("Unknown menu item"),
        }
    }
}

fn read_line_trimmed() -> Result<String> {
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

fn add_chatgpt_rules(input: &PathBuf, outbound: &str) -> Result<()> {
    let mut config = load_config(input)?;

    let domains = [
        "chatgpt.com",
        "openai.com",
        "oaistatic.com",
        "oaiusercontent.com",
        "chat.openai.com",
        "auth.openai.com",
        "cdn.oaistatic.com",
    ];

    for domain in domains {
        config
            .rules
            .retain(|rule| !(rule.rule_type == "domain_suffix" && rule.value == domain));

        config.rules.push(Rule {
            rule_type: "domain_suffix".to_string(),
            value: domain.to_string(),
            outbound: outbound.to_string(),
        });
    }

    validate_config(&config)?;
    write_config_toml(input, &config)?;

    println!("ChatGPT/OpenAI rules saved to {}", input.display());
    println!("Outbound: {}", outbound);

    Ok(())
}

fn handle_rule_command(command: RuleCommand) -> Result<()> {
    match command {
        RuleCommand::List { input } => {
            let config = load_config(&input)?;
            validate_config(&config)?;

            if config.rules.is_empty() {
                println!("No rules configured");
                return Ok(());
            }

            for (i, rule) in config.rules.iter().enumerate() {
                println!(
                    "[{}] {} {} -> {}",
                    i, rule.rule_type, rule.value, rule.outbound
                );
            }

            println!("final -> {}", config.general.final_outbound);
        }

        RuleCommand::Add {
            input,
            rule_type,
            value,
            outbound,
            output,
        } => {
            let mut config = load_config(&input)?;

            let old_len = config.rules.len();

            config
                .rules
                .retain(|rule| !(rule.rule_type == rule_type && rule.value == value));

            let removed_count = old_len - config.rules.len();

            config.rules.push(Rule {
                rule_type,
                value,
                outbound,
            });

            validate_config(&config)?;

            let out_path = output.as_deref().unwrap_or(&input);
            write_config_toml(out_path, &config)?;

            if removed_count == 0 {
                println!("Rule added");
            } else {
                println!("Rule replaced, removed {} old duplicate(s)", removed_count);
            }

            println!("Saved config: {}", out_path.display());
        }

        RuleCommand::Remove {
            input,
            index,
            output,
        } => {
            let mut config = load_config(&input)?;
            validate_config(&config)?;

            if index >= config.rules.len() {
                anyhow::bail!("Rule index {} does not exist", index);
            }

            let removed = config.rules.remove(index);

            let out_path = output.as_deref().unwrap_or(&input);
            write_config_toml(out_path, &config)?;

            println!(
                "Removed rule: {} {} -> {}",
                removed.rule_type, removed.value, removed.outbound
            );
            println!("Saved config: {}", out_path.display());
        }
    }

    Ok(())
}
