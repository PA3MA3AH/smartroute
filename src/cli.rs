use crate::{
    config::{Chain, LocalProfile, Rule, load_config, validate_config},
    daemon::run_daemon,
    diagnosis::{diagnose_ai_access, diagnose_site, watch_sites},
    picker::pick_node,
    runtime::{start_smartroute, status_smartroute, stop_smartroute},
    singbox::generate_singbox_config,
    subscription::import_url,
    tester::{auto_select_fastest, test_nodes},
    util::write_config_toml,
};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{self, ClearType},
};
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum UiLang {
    En,
    Ru,
}

#[derive(Clone, Copy)]
enum UiAction {
    StartDaemonSafe,
    StartDaemonFull,
    StartOnce,
    Stop,
    Status,
    DiagnoseChatGpt,
    DiagnoseCustom,
    DiagnoseAiAccess,
    SetChatGptRoute,
    ListRules,
    ImportSubscription,
    ChangeConfig,
    ToggleLanguage,
    AboutProxyVpn,
    CreateChainProxy,
    AssignDomainToChain,
    CreateLocalPortProfile,
    ListChainsAndProfiles,
    Exit,
}

struct UiItem {
    action: UiAction,
    en: &'static str,
    ru: &'static str,
    en_hint: &'static str,
    ru_hint: &'static str,
}

struct RawModeGuard;

impl RawModeGuard {
    fn new() -> Result<Self> {
        terminal::enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

fn run_ui(input: PathBuf) -> Result<()> {
    let mut input = input;
    let mut selected = 0usize;
    let mut lang = UiLang::Ru;
    let mut last_message: Option<String> = None;
    let items = ui_items();

    let _raw = RawModeGuard::new()?;

    loop {
        draw_tui(&input, selected, lang, &items, last_message.as_deref())?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Up => selected = selected.saturating_sub(1),
                KeyCode::Down => {
                    if selected + 1 < items.len() {
                        selected += 1;
                    }
                }
                KeyCode::Home => selected = 0,
                KeyCode::End => selected = items.len().saturating_sub(1),
                KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
                KeyCode::Char('j') => {
                    if selected + 1 < items.len() {
                        selected += 1;
                    }
                }
                KeyCode::Char('k') => selected = selected.saturating_sub(1),
                KeyCode::Enter => {
                    let action = items[selected].action;

                    if matches!(action, UiAction::Exit) {
                        return Ok(());
                    }

                    if matches!(action, UiAction::ToggleLanguage) {
                        lang = match lang {
                            UiLang::En => UiLang::Ru,
                            UiLang::Ru => UiLang::En,
                        };
                        last_message = Some(match lang {
                            UiLang::En => "Language switched to English".to_string(),
                            UiLang::Ru => "Язык переключён на русский".to_string(),
                        });
                        continue;
                    }

                    terminal::disable_raw_mode()?;
                    let result = run_ui_action(action, &mut input, lang);
                    terminal::enable_raw_mode()?;

                    match result {
                        Ok(message) => last_message = message,
                        Err(err) => last_message = Some(format!("ERROR: {err:#}")),
                    }
                }
                _ => {}
            }
        }
    }
}

fn ui_items() -> Vec<UiItem> {
    vec![
        UiItem { action: UiAction::StartDaemonSafe, en: "Start daemon: safe mode", ru: "Запустить daemon: безопасный режим", en_hint: "Auto-routes Discord/YouTube, keeps ChatGPT/OpenAI sticky.", ru_hint: "Авто-роутинг Discord/YouTube, ChatGPT/OpenAI не переключаются." },
        UiItem { action: UiAction::StartDaemonFull, en: "Start daemon: full preset", ru: "Запустить daemon: полный пресет", en_hint: "Runs the normal preset. Use carefully if you changed sticky policy.", ru_hint: "Запускает обычный пресет. Осторожно, если менял sticky-политику." },
        UiItem { action: UiAction::StartOnce, en: "Start SmartRoute once", ru: "Запустить SmartRoute один раз", en_hint: "Starts sing-box SOCKS5 on 127.0.0.1:1081 without auto-diagnose loop.", ru_hint: "Запускает sing-box SOCKS5 на 127.0.0.1:1081 без авто-диагностики." },
        UiItem { action: UiAction::Stop, en: "Stop SmartRoute", ru: "Остановить SmartRoute", en_hint: "Stops the running sing-box process managed by SmartRoute.", ru_hint: "Останавливает запущенный через SmartRoute процесс sing-box." },
        UiItem { action: UiAction::Status, en: "Status", ru: "Статус", en_hint: "Shows whether SmartRoute/sing-box is running.", ru_hint: "Показывает, запущен ли SmartRoute/sing-box." },
        UiItem { action: UiAction::DiagnoseChatGpt, en: "Diagnose ChatGPT now", ru: "Проверить ChatGPT сейчас", en_hint: "Manual check. Safe daemon will not auto-switch ChatGPT later.", ru_hint: "Ручная проверка. Безопасный daemon потом не будет менять ChatGPT." },
        UiItem { action: UiAction::DiagnoseCustom, en: "Diagnose custom domain", ru: "Проверить свой домен", en_hint: "Finds a working route for a domain you type.", ru_hint: "Ищет рабочий маршрут для домена, который ты введёшь." },
        UiItem { action: UiAction::DiagnoseAiAccess, en: "Diagnose AI access", ru: "Проверить доступ к AI-сервисам", en_hint: "Checks ChatGPT, Claude, Gemini, Copilot, Venice, Perplexity and detects region-block pages.", ru_hint: "Проверяет ChatGPT, Claude, Gemini, Copilot, Venice, Perplexity и ловит региональные заглушки." },
        UiItem { action: UiAction::SetChatGptRoute, en: "Set ChatGPT/OpenAI route", ru: "Закрепить маршрут ChatGPT/OpenAI", en_hint: "Writes all OpenAI-related rules to one outbound tag.", ru_hint: "Прописывает все OpenAI-домены на один outbound tag." },
        UiItem { action: UiAction::ListRules, en: "List routing rules", ru: "Показать правила роутинга", en_hint: "Shows domain rules from the current config.", ru_hint: "Показывает domain rules из текущего конфига." },
        UiItem { action: UiAction::ImportSubscription, en: "Import proxy subscription URL", ru: "Импортировать прокси по ссылке", en_hint: "Creates a config from a subscription URL.", ru_hint: "Создаёт конфиг из subscription-ссылки." },
        UiItem { action: UiAction::ChangeConfig, en: "Change config path", ru: "Сменить путь к конфигу", en_hint: "Current config file used by UI actions.", ru_hint: "Текущий config-файл для действий в UI." },
        UiItem { action: UiAction::ToggleLanguage, en: "Language: English / Русский", ru: "Язык: Русский / English", en_hint: "Switch UI language.", ru_hint: "Переключить язык интерфейса." },
        UiItem { action: UiAction::CreateChainProxy, en: "Create chain proxy", ru: "Создать chain proxy", en_hint: "Builds app -> SmartRoute -> proxy A -> proxy B -> site.", ru_hint: "Создаёт цепочку: приложение -> SmartRoute -> proxy A -> proxy B -> сайт." },
        UiItem { action: UiAction::AssignDomainToChain, en: "Assign site/domain to chain", ru: "Назначить сайт/домен на chain", en_hint: "Routes a specific domain through an existing chain tag.", ru_hint: "Направляет конкретный домен через существующий chain tag." },
        UiItem { action: UiAction::CreateLocalPortProfile, en: "Create app proxy port", ru: "Создать proxy-порт для приложения", en_hint: "Creates another local SOCKS port, e.g. 1082 for one app/profile.", ru_hint: "Создаёт отдельный локальный SOCKS-порт, например 1082 для приложения/профиля." },
        UiItem { action: UiAction::ListChainsAndProfiles, en: "List chains and app ports", ru: "Показать chains и app-порты", en_hint: "Shows configured chain proxies and local per-app SOCKS ports.", ru_hint: "Показывает chain proxies и локальные SOCKS-порты под приложения." },
        UiItem { action: UiAction::AboutProxyVpn, en: "What is this? Proxy vs VPN", ru: "Что это? Прокси vs VPN", en_hint: "Explains what SmartRoute does and does not do.", ru_hint: "Объясняет, что SmartRoute делает и чем он не является." },
        UiItem { action: UiAction::Exit, en: "Exit", ru: "Выход", en_hint: "Close the TUI.", ru_hint: "Закрыть TUI." },
    ]
}

fn draw_tui(input: &PathBuf, selected: usize, lang: UiLang, items: &[UiItem], last_message: Option<&str>) -> Result<()> {
    let mut out = io::stdout();
    let (_, height) = terminal::size().unwrap_or((100, 30));
    let mut row: u16 = 0;

    execute!(out, terminal::Clear(ClearType::All), cursor::MoveTo(0, 0))?;

    draw_line(&mut out, row, "SmartRoute", Color::Cyan, true)?;
    row += 1;
    draw_line(
        &mut out,
        row,
        match lang {
            UiLang::En => "Local SOCKS5 proxy router. It is NOT a VPN.",
            UiLang::Ru => "Локальный SOCKS5 proxy-router. Это НЕ VPN.",
        },
        Color::White,
        false,
    )?;
    row += 1;
    draw_line(&mut out, row, "────────────────────────────────────────────────────────", Color::DarkGrey, false)?;
    row += 1;

    draw_line(
        &mut out,
        row,
        &format!("{} {}", match lang { UiLang::En => "Config:", UiLang::Ru => "Конфиг:" }, input.display()),
        Color::Grey,
        false,
    )?;
    row += 1;
    draw_line(&mut out, row, "SOCKS5 default: 127.0.0.1:1081", Color::Grey, false)?;
    row += 1;
    draw_line(
        &mut out,
        row,
        match lang {
            UiLang::En => "Keys: ↑/↓ or k/j = move, Enter = run, q/Esc = exit",
            UiLang::Ru => "Клавиши: ↑/↓ или k/j = выбор, Enter = выполнить, q/Esc = выход",
        },
        Color::Grey,
        false,
    )?;
    row += 2;

    let max_menu_rows = height.saturating_sub(8) as usize;
    let visible = items.len().min(max_menu_rows.max(1));
    let offset = if selected >= visible { selected + 1 - visible } else { 0 };

    for (idx, item) in items.iter().enumerate().skip(offset).take(visible) {
        let title = match lang { UiLang::En => item.en, UiLang::Ru => item.ru };
        if idx == selected {
            draw_line(&mut out, row, &format!("> {title}"), Color::Yellow, true)?;
        } else {
            draw_line(&mut out, row, &format!("  {title}"), Color::White, false)?;
        }
        row += 1;
    }

    let footer_row = height.saturating_sub(4);
    draw_line(&mut out, footer_row, "────────────────────────────────────────────────────────", Color::DarkGrey, false)?;
    let hint = match lang { UiLang::En => items[selected].en_hint, UiLang::Ru => items[selected].ru_hint };
    draw_line(
        &mut out,
        footer_row + 1,
        &format!("{} {}", match lang { UiLang::En => "Hint:", UiLang::Ru => "Подсказка:" }, hint),
        Color::Cyan,
        false,
    )?;

    if let Some(message) = last_message {
        draw_line(&mut out, footer_row + 2, message, Color::Green, false)?;
    }

    out.flush()?;
    Ok(())
}

fn draw_line(out: &mut io::Stdout, row: u16, text: &str, color: Color, bold: bool) -> Result<()> {
    execute!(out, cursor::MoveTo(0, row), terminal::Clear(ClearType::CurrentLine))?;
    if bold {
        execute!(out, SetAttribute(Attribute::Bold))?;
    }
    execute!(out, SetForegroundColor(color), Print(text), ResetColor, SetAttribute(Attribute::Reset))?;
    Ok(())
}

fn run_ui_action(action: UiAction, input: &mut PathBuf, lang: UiLang) -> Result<Option<String>> {
    clear_for_command()?;

    match action {
        UiAction::StartDaemonSafe => {
            println!("{}", match lang { UiLang::En => "Starting daemon in safe mode. Press Ctrl+C to stop.", UiLang::Ru => "Запуск daemon в безопасном режиме. Ctrl+C чтобы остановить." });
            let domains = vec!["chatgpt.com".to_string(), "discord.com".to_string(), "youtube.com".to_string()];
            run_daemon(input, 2, domains, 300, 8, 12, 3, 25, false)?;
            Ok(None)
        }
        UiAction::StartDaemonFull => {
            println!("{}", match lang { UiLang::En => "Starting daemon full preset. Press Ctrl+C to stop.", UiLang::Ru => "Запуск daemon с полным пресетом. Ctrl+C чтобы остановить." });
            let domains = vec!["chatgpt.com".to_string(), "discord.com".to_string(), "youtube.com".to_string()];
            run_daemon(input, 2, domains, 300, 8, 12, 3, 25, true)?;
            Ok(None)
        }
        UiAction::StartOnce => {
            start_smartroute(input)?;
            pause(lang)?;
            Ok(Some(match lang { UiLang::En => "SmartRoute started.".to_string(), UiLang::Ru => "SmartRoute запущен.".to_string() }))
        }
        UiAction::Stop => {
            stop_smartroute()?;
            pause(lang)?;
            Ok(Some(match lang { UiLang::En => "SmartRoute stopped.".to_string(), UiLang::Ru => "SmartRoute остановлен.".to_string() }))
        }
        UiAction::Status => {
            status_smartroute()?;
            pause(lang)?;
            Ok(None)
        }
        UiAction::DiagnoseChatGpt => {
            diagnose_site(input, None, "chatgpt.com", 8, 12, 3, 25, false)?;
            pause(lang)?;
            Ok(Some(match lang { UiLang::En => "ChatGPT diagnose finished.".to_string(), UiLang::Ru => "Проверка ChatGPT завершена.".to_string() }))
        }
        UiAction::DiagnoseCustom => {
            let domain = prompt_line(match lang { UiLang::En => "Domain: ", UiLang::Ru => "Домен: " })?;
            if domain.is_empty() {
                return Ok(Some(match lang { UiLang::En => "Cancelled: empty domain.".to_string(), UiLang::Ru => "Отменено: пустой домен.".to_string() }));
            }
            diagnose_site(input, None, &domain, 8, 12, 3, 25, false)?;
            pause(lang)?;
            Ok(Some(format!("{}: {}", match lang { UiLang::En => "Diagnose finished", UiLang::Ru => "Проверка завершена" }, domain)))
        }
        UiAction::DiagnoseAiAccess => {
            diagnose_ai_access(input, 10, 12, 2, 25)?;
            pause(lang)?;
            Ok(Some(match lang { UiLang::En => "AI access diagnose finished.".to_string(), UiLang::Ru => "Проверка AI-доступа завершена.".to_string() }))
        }
        UiAction::SetChatGptRoute => {
            let outbound = prompt_line(match lang { UiLang::En => "Outbound tag for ChatGPT/OpenAI: ", UiLang::Ru => "Outbound tag для ChatGPT/OpenAI: " })?;
            if outbound.is_empty() {
                return Ok(Some(match lang { UiLang::En => "Cancelled: empty outbound.".to_string(), UiLang::Ru => "Отменено: пустой outbound.".to_string() }));
            }
            add_chatgpt_rules(input, &outbound)?;
            pause(lang)?;
            Ok(Some(format!("ChatGPT/OpenAI -> {outbound}")))
        }
        UiAction::ListRules => {
            handle_rule_command(RuleCommand::List { input: input.clone() })?;
            pause(lang)?;
            Ok(None)
        }
        UiAction::ImportSubscription => {
            let url = prompt_line(match lang { UiLang::En => "Subscription URL: ", UiLang::Ru => "Subscription URL: " })?;
            if url.is_empty() {
                return Ok(Some(match lang { UiLang::En => "Cancelled: empty URL.".to_string(), UiLang::Ru => "Отменено: пустая ссылка.".to_string() }));
            }
            let output = prompt_line(match lang { UiLang::En => "Output config path [imported.toml]: ", UiLang::Ru => "Куда сохранить конфиг [imported.toml]: " })?;
            let output = if output.is_empty() { PathBuf::from("imported.toml") } else { PathBuf::from(output) };
            import_url(&url, &output)?;
            *input = output.clone();
            pause(lang)?;
            Ok(Some(format!("{} {}", match lang { UiLang::En => "Imported to", UiLang::Ru => "Импортировано в" }, output.display())))
        }
        UiAction::ChangeConfig => {
            let new_path = prompt_line(match lang { UiLang::En => "New config path: ", UiLang::Ru => "Новый путь к конфигу: " })?;
            if !new_path.is_empty() {
                *input = PathBuf::from(new_path);
            }
            Ok(Some(format!("{} {}", match lang { UiLang::En => "Config:", UiLang::Ru => "Конфиг:" }, input.display())))
        }
        UiAction::CreateChainProxy => {
            create_chain_proxy(input, lang)?;
            pause(lang)?;
            Ok(Some(match lang { UiLang::En => "Chain proxy saved.".to_string(), UiLang::Ru => "Chain proxy сохранён.".to_string() }))
        }
        UiAction::AssignDomainToChain => {
            assign_domain_to_chain(input, lang)?;
            pause(lang)?;
            Ok(Some(match lang { UiLang::En => "Domain rule saved.".to_string(), UiLang::Ru => "Правило домена сохранено.".to_string() }))
        }
        UiAction::CreateLocalPortProfile => {
            create_local_port_profile(input, lang)?;
            pause(lang)?;
            Ok(Some(match lang { UiLang::En => "Local app proxy port saved.".to_string(), UiLang::Ru => "Локальный proxy-порт сохранён.".to_string() }))
        }
        UiAction::ListChainsAndProfiles => {
            list_chains_and_profiles(input)?;
            pause(lang)?;
            Ok(None)
        }
        UiAction::AboutProxyVpn => {
            print_about(lang);
            pause(lang)?;
            Ok(None)
        }
        UiAction::ToggleLanguage | UiAction::Exit => Ok(None),
    }
}

fn clear_for_command() -> Result<()> {
    let mut out = io::stdout();
    execute!(out, terminal::Clear(ClearType::All), cursor::MoveTo(0, 0))?;
    Ok(())
}

fn prompt_line(prompt: &str) -> Result<String> {
    print!("{}", prompt);
    io::stdout().flush()?;
    read_line_trimmed()
}

fn read_line_trimmed() -> Result<String> {
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

fn pause(lang: UiLang) -> Result<()> {
    println!();
    println!("{}", match lang { UiLang::En => "Press Enter to return to menu...", UiLang::Ru => "Нажми Enter, чтобы вернуться в меню..." });
    let _ = read_line_trimmed()?;
    Ok(())
}

fn print_about(lang: UiLang) {
    match lang {
        UiLang::En => {
            println!("SmartRoute is a local SOCKS5 proxy router, not a VPN.");
            println!();
            println!("Proxy:");
            println!("  Routes traffic only from apps configured to use it, for example a browser.");
            println!("  SmartRoute exposes SOCKS5 at 127.0.0.1:1081.");
            println!();
            println!("VPN:");
            println!("  Usually routes the whole system or network interface through a tunnel.");
            println!();
            println!("SmartRoute chooses working outbound nodes per domain and writes sing-box rules.");
        }
        UiLang::Ru => {
            println!("SmartRoute — это локальный SOCKS5 proxy-router, а не VPN.");
            println!();
            println!("Прокси:");
            println!("  Пропускает трафик только тех приложений, где ты указал прокси, например браузера.");
            println!("  SmartRoute поднимает SOCKS5 на 127.0.0.1:1081.");
            println!();
            println!("VPN:");
            println!("  Обычно гонит весь системный трафик или сетевой интерфейс через туннель.");
            println!();
            println!("SmartRoute выбирает рабочие outbound-ноды по доменам и пишет правила sing-box.");
        }
    }
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

fn create_chain_proxy(input: &PathBuf, lang: UiLang) -> Result<()> {
    let mut config = load_config(input)?;

    println!("{}", match lang {
        UiLang::En => "Chain proxy: app -> SmartRoute -> first outbound -> second outbound -> site",
        UiLang::Ru => "Chain proxy: приложение -> SmartRoute -> первый outbound -> второй outbound -> сайт",
    });
    println!();
    print_available_outbounds(&config);
    println!();

    let tag = prompt_line(match lang { UiLang::En => "New chain tag: ", UiLang::Ru => "Tag новой цепочки: " })?;
    if tag.is_empty() {
        anyhow::bail!("empty chain tag");
    }

    let first = prompt_line(match lang { UiLang::En => "First outbound tag: ", UiLang::Ru => "Первый outbound tag: " })?;
    let second = prompt_line(match lang { UiLang::En => "Second outbound tag: ", UiLang::Ru => "Второй outbound tag: " })?;
    let third = prompt_line(match lang { UiLang::En => "Optional third outbound tag [Enter to skip]: ", UiLang::Ru => "Третий outbound tag, если нужен [Enter чтобы пропустить]: " })?;

    let mut outbounds = vec![first, second];
    if !third.is_empty() {
        outbounds.push(third);
    }

    config.chains.retain(|chain| chain.tag != tag);
    config.chains.push(Chain { tag: tag.clone(), outbounds });

    validate_config(&config)?;
    write_config_toml(input, &config)?;

    println!("{}: {}", match lang { UiLang::En => "Saved chain", UiLang::Ru => "Сохранена цепочка" }, tag);
    println!("{}", match lang {
        UiLang::En => "Use 'Assign site/domain to chain' to route a site through it.",
        UiLang::Ru => "Используй 'Назначить сайт/домен на chain', чтобы направить сайт через неё.",
    });

    Ok(())
}

fn assign_domain_to_chain(input: &PathBuf, lang: UiLang) -> Result<()> {
    let mut config = load_config(input)?;

    list_chains_and_profiles(input)?;
    println!();

    let domain = prompt_line(match lang { UiLang::En => "Domain suffix, e.g. example.com: ", UiLang::Ru => "Доменный суффикс, например example.com: " })?;
    if domain.is_empty() {
        anyhow::bail!("empty domain");
    }

    let outbound = prompt_line(match lang { UiLang::En => "Chain/outbound tag: ", UiLang::Ru => "Chain/outbound tag: " })?;
    if outbound.is_empty() {
        anyhow::bail!("empty outbound");
    }

    config
        .rules
        .retain(|rule| !(rule.rule_type == "domain_suffix" && rule.value == domain));

    config.rules.push(Rule {
        rule_type: "domain_suffix".to_string(),
        value: domain.clone(),
        outbound: outbound.clone(),
    });

    validate_config(&config)?;
    write_config_toml(input, &config)?;

    println!("domain_suffix {} -> {}", domain, outbound);
    Ok(())
}

fn create_local_port_profile(input: &PathBuf, lang: UiLang) -> Result<()> {
    let mut config = load_config(input)?;

    println!("{}", match lang {
        UiLang::En => "App proxy profile = separate local SOCKS port for one app/browser profile.",
        UiLang::Ru => "Профиль приложения = отдельный локальный SOCKS-порт для приложения/профиля браузера.",
    });
    println!("{}", match lang {
        UiLang::En => "Example: Zen uses 127.0.0.1:1082, Telegram uses 127.0.0.1:1083.",
        UiLang::Ru => "Пример: Zen использует 127.0.0.1:1082, Telegram использует 127.0.0.1:1083.",
    });
    println!();
    print_available_outbounds(&config);
    println!();

    let tag = prompt_line(match lang { UiLang::En => "Profile tag: ", UiLang::Ru => "Tag профиля: " })?;
    if tag.is_empty() {
        anyhow::bail!("empty profile tag");
    }

    let port_raw = prompt_line(match lang { UiLang::En => "Local SOCKS port, e.g. 1082: ", UiLang::Ru => "Локальный SOCKS-порт, например 1082: " })?;
    let listen_port: u16 = port_raw.parse().context("invalid port")?;

    let outbound = prompt_line(match lang { UiLang::En => "Outbound/chain tag for this port: ", UiLang::Ru => "Outbound/chain tag для этого порта: " })?;
    if outbound.is_empty() {
        anyhow::bail!("empty outbound");
    }

    config.local_profiles.retain(|profile| profile.tag != tag);
    config.local_profiles.push(LocalProfile {
        tag: tag.clone(),
        listen: "127.0.0.1".to_string(),
        listen_port,
        outbound: outbound.clone(),
    });

    validate_config(&config)?;
    write_config_toml(input, &config)?;

    println!("127.0.0.1:{} -> {}", listen_port, outbound);
    Ok(())
}

fn list_chains_and_profiles(input: &PathBuf) -> Result<()> {
    let config = load_config(input)?;
    validate_config(&config)?;

    println!("Chains:");
    if config.chains.is_empty() {
        println!("  none");
    } else {
        for chain in &config.chains {
            println!("  {} = {}", chain.tag, chain.outbounds.join(" -> "));
        }
    }

    println!();
    println!("Local app SOCKS ports:");
    if config.local_profiles.is_empty() {
        println!("  none");
    } else {
        for profile in &config.local_profiles {
            println!(
                "  {}: {}:{} -> {}",
                profile.tag, profile.listen, profile.listen_port, profile.outbound
            );
        }
    }

    Ok(())
}

fn print_available_outbounds(config: &crate::config::SmartRouteConfig) {
    println!("Available base outbounds:");
    println!("  direct");
    println!("  block");
    for node in &config.nodes {
        println!("  {}", node.tag);
    }
    if !config.chains.is_empty() {
        println!("Existing chains:");
        for chain in &config.chains {
            println!("  {}", chain.tag);
        }
    }
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
