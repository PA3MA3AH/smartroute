use crate::{
    autostart::{disable_autostart, enable_autostart, status_autostart},
    backup::{backup_config, list_backups, restore_backup},
    config::{load_config, validate_config},
    daemon::run_daemon,
    diagnosis::{diagnose_site, watch_sites},
    dnstest::run_dns_test,
    doctor::doctor_config,
    health::{health_check, repair_smartroute},
    killswitch::{disable_killswitch, enable_killswitch, status_killswitch},
    leaktest::run_leak_test,
    mask::{list_masks, set_mask},
    merge::merge_nodes,
    picker::pick_node,
    resolve::resolve_domains_to_ip,
    runtime::{start_smartroute, status_smartroute, stop_smartroute},
    singbox::generate_singbox_config,
    subscription::import_url,
    tester::{auto_select_fastest, test_nodes},
    whitelist::{list_whitelist_masks, run_whitelist_test},
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
    MergeNodes {
        base: PathBuf,
        nodes: PathBuf,

        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    Backup {
        input: PathBuf,
    },

    Backups {
        input: Option<PathBuf>,
    },

    Restore {
        input: PathBuf,

        #[arg(long)]
        file: Option<PathBuf>,

        #[arg(long, default_value_t = false)]
        latest: bool,
    },

    Doctor {
        input: PathBuf,

        #[arg(long, default_value_t = false)]
        strict: bool,
    },

    Health {
        input: PathBuf,

        #[arg(long, default_value = "google.com")]
        domain: String,

        #[arg(long, default_value_t = false)]
        full: bool,
    },

    Repair {
        input: PathBuf,

        #[arg(long, default_value = "google.com")]
        domain: String,

        #[arg(long, default_value_t = false)]
        full: bool,
    },

    Whitelist {
        #[command(subcommand)]
        command: WhitelistCommand,
    },

    LeakTest {
        input: PathBuf,

        #[arg(long, default_value = "google.com")]
        domain: String,

        #[arg(short, long)]
        interface: Option<String>,
    },

    DnsTest {
        input: PathBuf,

        #[arg(long, default_value = "youtube.com")]
        domain: String,

        #[arg(short, long)]
        interface: Option<String>,

        #[arg(long, default_value_t = false)]
        strict: bool,
    },

    Mask {
        #[command(subcommand)]
        command: MaskCommand,
    },

    KillSwitch {
        #[command(subcommand)]
        command: KillSwitchCommand,
    },

    Autostart {
        #[command(subcommand)]
        command: AutostartCommand,
    },

    ResolveDomains {
        input: PathBuf,
    },

    Daemon {
        input: PathBuf,

        #[arg(short, long, default_value_t = 2)]
        interval: u64,

        #[arg(short, long)]
        domain: Vec<String>,

        #[arg(long, default_value_t = 300)]
        diagnose_interval: u64,

        #[arg(long, default_value_t = 30)]
        heal_interval: u64,

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
enum WhitelistCommand {
    List {
        input: PathBuf,
    },

    Test {
        input: PathBuf,

        #[arg(long, default_value = "youtube.com")]
        domain: String,

        #[arg(short, long)]
        interface: Option<String>,
    },
}

#[derive(Subcommand)]
enum MaskCommand {
    List {
        input: PathBuf,
    },

    Set {
        input: PathBuf,
        tag: String,

        #[arg(long)]
        server_name: Option<String>,

        #[arg(long)]
        fingerprint: Option<String>,

        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum KillSwitchCommand {
    Enable {
        input: PathBuf,

        #[arg(long, default_value_t = true)]
        smart: bool,
    },
    Disable,
    Status,
}

#[derive(Subcommand)]
enum AutostartCommand {
    Enable { input: PathBuf },
    Disable,
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
        Commands::MergeNodes {
            base,
            nodes,
            output,
        } => {
            merge_nodes(&base, &nodes, output.as_deref())?;
        }

        Commands::Backup { input } => {
            backup_config(&input)?;
        }

        Commands::Backups { input } => {
            list_backups(input.as_deref())?;
        }

        Commands::Restore {
            input,
            file,
            latest: _,
        } => {
            restore_backup(&input, file.as_deref())?;
        }

        Commands::Doctor { input, strict } => {
            doctor_config(&input, strict)?;
        }

        Commands::Whitelist { command } => match command {
            WhitelistCommand::List { input } => {
                list_whitelist_masks(&input)?;
            }

            WhitelistCommand::Test {
                input,
                domain,
                interface,
            } => {
                run_whitelist_test(&input, &domain, interface.as_deref())?;
            }
        },

        Commands::Health {
            input,
            domain,
            full,
        } => {
            health_check(&input, &domain, full)?;
        }

        Commands::Repair {
            input,
            domain,
            full,
        } => {
            repair_smartroute(&input, &domain, full)?;
        }

        Commands::LeakTest {
            input,
            domain,
            interface,
        } => {
            run_leak_test(&input, &domain, interface.as_deref())?;
        }

        Commands::DnsTest {
            input,
            domain,
            interface,
            strict,
        } => {
            run_dns_test(&input, &domain, interface.as_deref(), strict)?;
        }

        Commands::Mask { command } => match command {
            MaskCommand::List { input } => {
                list_masks(&input)?;
            }

            MaskCommand::Set {
                input,
                tag,
                server_name,
                fingerprint,
                output,
            } => {
                set_mask(
                    &input,
                    &tag,
                    server_name.as_deref(),
                    fingerprint.as_deref(),
                    output.as_deref(),
                )?;
            }
        },

        Commands::KillSwitch { command } => match command {
            KillSwitchCommand::Enable { input, smart } => {
                enable_killswitch(&input, smart)?;
            }
            KillSwitchCommand::Disable => {
                disable_killswitch()?;
            }
            KillSwitchCommand::Status => {
                status_killswitch()?;
            }
        },

        Commands::Autostart { command } => match command {
            AutostartCommand::Enable { input } => {
                enable_autostart(&input)?;
            }
            AutostartCommand::Disable => {
                disable_autostart()?;
            }
            AutostartCommand::Status => {
                status_autostart()?;
            }
        },

        Commands::ResolveDomains { input } => {
            let changed = resolve_domains_to_ip(&input)?;
            println!("Resolved {} domain node(s)", changed);
        }

        Commands::Daemon {
            input,
            interval,
            domain,
            diagnose_interval,
            heal_interval,
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
                heal_interval,
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
            crate::tui::run_tui(input)?;
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
                println!("[{}] {} {} -> {}", i, rule.rule_type, rule.value, rule.outbound);
            }
            println!("final -> {}", config.general.final_outbound);
        }

        RuleCommand::Add { input, rule_type, value, outbound, output } => {
            let mut config = load_config(&input)?;
            let old_len = config.rules.len();
            config.rules.retain(|r| !(r.rule_type == rule_type && r.value == value));
            let removed = old_len - config.rules.len();
            config.rules.push(crate::config::Rule { rule_type, value, outbound });
            validate_config(&config)?;
            let out = output.as_deref().unwrap_or(&input);
            crate::util::write_config_toml(out, &config)?;
            if removed == 0 { println!("Rule added"); } else { println!("Rule replaced ({} removed)", removed); }
            println!("Saved: {}", out.display());
        }

        RuleCommand::Remove { input, index, output } => {
            let mut config = load_config(&input)?;
            validate_config(&config)?;
            if index >= config.rules.len() {
                anyhow::bail!("Rule index {} does not exist", index);
            }
            let removed = config.rules.remove(index);
            let out = output.as_deref().unwrap_or(&input);
            crate::util::write_config_toml(out, &config)?;
            println!("Removed: {} {} -> {}", removed.rule_type, removed.value, removed.outbound);
            println!("Saved: {}", out.display());
        }
    }
    Ok(())
}
