// tui.rs — 16-item TUI, identical engine to cli.rs run_ui, different item list.
// Opened via `smartroute testui`.

use crate::{
    config::{LocalProfile, Rule, load_config, validate_config},
    daemon::run_daemon,
    killswitch::{disable_killswitch, enable_killswitch},
    leaktest::run_leak_test,
    runtime::{start_smartroute, stop_smartroute},
    subscription::import_url,
    util::write_config_toml,
};
use anyhow::{Context, Result};
use crossterm::{
    cursor,
    event::{self, Event, KeyCode},
    execute,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor},
    terminal::{self, ClearType},
};
use std::{
    io::{self, Write},
    net::{TcpStream, ToSocketAddrs},
    path::PathBuf,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum UiLang {
    En,
    Ru,
}

#[derive(Clone, Copy)]
enum UiAction {
    // 1-4: start/stop
    StartOnce,
    StartDaemonSafe,
    StartDaemonFull,
    Stop,
    // 5: server list with ping
    ListServers,
    // 6: set global server/chain
    SetGlobal,
    // 7-8: kill-switch (masking)
    KillSwitchEnable,
    KillSwitchDisable,
    // 9-10: rules
    SiteRule,
    AppRule,
    // 11-12: show/delete rules
    ShowRules,
    DeleteRule,
    // 13: leak test
    LeakTest,
    // 14: language
    ToggleLanguage,
    // 15: kill-switch status/toggle menu
    KillSwitchMenu,
    // 16: exit
    Exit,
}

struct UiItem {
    action: UiAction,
    en: &'static str,
    ru: &'static str,
    en_hint: &'static str,
    ru_hint: &'static str,
}

fn ui_items() -> Vec<UiItem> {
    vec![
        UiItem {
            action: UiAction::StartOnce,
            en: "1. Start",
            ru: "1. Запуск",
            en_hint: "Starts local SOCKS5 router on 127.0.0.1:1081.",
            ru_hint: "Запускает локальный SOCKS5-роутер на 127.0.0.1:1081.",
        },
        UiItem {
            action: UiAction::StartDaemonSafe,
            en: "2. Start daemon — safe mode",
            ru: "2. Запустить daemon — безопасный режим",
            en_hint: "Runs SmartRoute with periodic checks and conservative switching.",
            ru_hint: "Запускает SmartRoute с периодическими проверками и осторожным переключением.",
        },
        UiItem {
            action: UiAction::StartDaemonFull,
            en: "3. Start daemon — full preset",
            ru: "3. Запустить daemon — полный пресет",
            en_hint: "Runs SmartRoute with automatic route updates and diagnose.",
            ru_hint: "Запускает SmartRoute с автоматическим обновлением маршрутов и диагностикой.",
        },
        UiItem {
            action: UiAction::Stop,
            en: "4. Stop",
            ru: "4. Остановить",
            en_hint: "Stops the running SmartRoute/sing-box process.",
            ru_hint: "Останавливает запущенный SmartRoute/sing-box.",
        },
        UiItem {
            action: UiAction::ListServers,
            en: "5. Server list (name, protocol, ping)",
            ru: "5. Список серверов (имя, протокол, пинг)",
            en_hint: "Shows all nodes from config with protocol and TCP ping to each.",
            ru_hint: "Показывает все ноды из конфига с протоколом и TCP-пингом до каждой.",
        },
        UiItem {
            action: UiAction::SetGlobal,
            en: "6. Set global server / chain",
            ru: "6. Выбрать глобальный сервер / chain",
            en_hint: "All traffic without a specific rule goes through this server or chain.",
            ru_hint: "Весь трафик без отдельного правила идёт через этот сервер или chain.",
        },
        UiItem {
            action: UiAction::KillSwitchEnable,
            en: "7. Enable traffic masking (kill-switch on)",
            ru: "7. Включить маскировку трафика (kill-switch вкл)",
            en_hint: "Blocks all direct traffic via nftables. Only proxy traffic is allowed.",
            ru_hint: "Блокирует весь прямой трафик через nftables. Разрешён только proxy-трафик.",
        },
        UiItem {
            action: UiAction::KillSwitchDisable,
            en: "8. Disable traffic masking (kill-switch off)",
            ru: "8. Выключить маскировку трафика (kill-switch выкл)",
            en_hint: "Removes nftables kill-switch rules. Direct traffic is allowed again.",
            ru_hint: "Удаляет правила nftables kill-switch. Прямой трафик снова разрешён.",
        },
        UiItem {
            action: UiAction::SiteRule,
            en: "9. Set server / chain for site",
            ru: "9. Выбрать сервер / chain для сайта",
            en_hint: "Routes a domain suffix through a specific server or chain proxy.",
            ru_hint: "Направляет доменный суффикс через конкретный сервер или chain proxy.",
        },
        UiItem {
            action: UiAction::AppRule,
            en: "10. Set server / chain for app",
            ru: "10. Выбрать сервер / chain для приложения",
            en_hint: "Creates a separate local SOCKS5 port for an app. Set it in app proxy settings.",
            ru_hint: "Создаёт отдельный локальный SOCKS5-порт для приложения. Укажи его в настройках прокси приложения.",
        },
        UiItem {
            action: UiAction::ShowRules,
            en: "11. Show all rules and chains",
            ru: "11. Показать все правила и chains",
            en_hint: "Lists all routing rules, chains and the global (final) outbound.",
            ru_hint: "Показывает все правила маршрутизации, chains и глобальный (final) outbound.",
        },
        UiItem {
            action: UiAction::DeleteRule,
            en: "12. Delete rule / chain",
            ru: "12. Удалить правило / chain",
            en_hint: "Removes a rule by index or a chain by tag. Traffic falls back to global.",
            ru_hint: "Удаляет правило по номеру или chain по тегу. Трафик идёт через глобальный сервер.",
        },
        UiItem {
            action: UiAction::LeakTest,
            en: "13. Leak test (DNS, IP, location)",
            ru: "13. Проверка на утечки (DNS, IP, местоположение)",
            en_hint: "Checks kill-switch, direct blocking, SOCKS route, SNI and proxy destinations.",
            ru_hint: "Проверяет kill-switch, блокировку direct, SOCKS-маршрут, SNI и IP proxy-нод.",
        },
        UiItem {
            action: UiAction::ToggleLanguage,
            en: "14. Language: English / Русский",
            ru: "14. Язык: Русский / English",
            en_hint: "Switch UI language between Russian and English.",
            ru_hint: "Переключить язык интерфейса между русским и английским.",
        },
        UiItem {
            action: UiAction::KillSwitchMenu,
            en: "15. Kill-switch (status / toggle)",
            ru: "15. Kill-switch (статус / переключить)",
            en_hint: "Shows current kill-switch status and lets you enable or disable it.",
            ru_hint: "Показывает текущий статус kill-switch и позволяет включить или выключить его.",
        },
        UiItem {
            action: UiAction::Exit,
            en: "16. Exit",
            ru: "16. Выход",
            en_hint: "Close the TUI and return to terminal.",
            ru_hint: "Закрыть TUI и вернуться в терминал.",
        },
    ]
}

// ── Status helpers ────────────────────────────────────────────────────────────

fn singbox_running() -> bool {
    std::process::Command::new("pgrep")
        .args(["-x", "sing-box"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn killswitch_active() -> bool {
    std::process::Command::new("nft")
        .args(["list", "table", "inet", "smartroute"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn ping_global_ms(config_path: &std::path::Path) -> Option<u64> {
    let config = load_config(config_path).ok()?;
    let final_tag = &config.general.final_outbound;

    let node = config.nodes.iter().find(|n| &n.tag == final_tag).or_else(|| {
        config.chains.iter()
            .find(|c| &c.tag == final_tag)
            .and_then(|ch| ch.outbounds.last())
            .and_then(|last| config.nodes.iter().find(|n| &n.tag == last))
    })?;

    let addr = format!("{}:{}", node.server, node.port);
    // Start timer BEFORE DNS resolve so we measure total connection time
    let t = Instant::now();
    let sa = addr.to_socket_addrs().ok()?.next()?;
    TcpStream::connect_timeout(&sa, Duration::from_secs(3)).ok()?;
    Some(t.elapsed().as_millis() as u64)
}

// ── Raw mode guard (identical to cli.rs) ─────────────────────────────────────

struct RawModeGuard;

impl RawModeGuard {
    fn new() -> Result<Self> {
        terminal::enable_raw_mode()?;
        let mut out = io::stdout();
        execute!(
            out,
            terminal::EnterAlternateScreen,
            cursor::Hide,
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        )?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let mut out = io::stdout();
        let _ = execute!(
            out,
            cursor::Show,
            terminal::LeaveAlternateScreen,
            terminal::Clear(ClearType::All),
            cursor::MoveTo(0, 0)
        );
        let _ = terminal::disable_raw_mode();
    }
}

// ── First-run setup ───────────────────────────────────────────────────────────

fn first_run_setup(lang: UiLang) -> Result<PathBuf> {
    println!("{}", match lang {
        UiLang::En => "SmartRoute — first run",
        UiLang::Ru => "SmartRoute — первый запуск",
    });
    println!();
    println!("{}", match lang {
        UiLang::En => "Enter config path (e.g. imported.toml) or subscription URL (https://...):",
        UiLang::Ru => "Введите путь к конфигу (например imported.toml) или subscription URL (https://...):",
    });
    print!("> ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();

    if input.is_empty() {
        anyhow::bail!(match lang {
            UiLang::En => "Cancelled: empty input",
            UiLang::Ru => "Отменено: пустой ввод",
        });
    }

    if input.starts_with("http://") || input.starts_with("https://") {
        let output = PathBuf::from("imported.toml");
        println!("{}", match lang {
            UiLang::En => "Importing subscription...",
            UiLang::Ru => "Импорт подписки...",
        });
        import_url(input, &output)?;
        println!("{}", match lang {
            UiLang::En => "Done. Config saved to imported.toml",
            UiLang::Ru => "Готово. Конфиг сохранён в imported.toml",
        });
        Ok(output)
    } else {
        let path = PathBuf::from(input);
        if !path.exists() {
            anyhow::bail!(match lang {
                UiLang::En => "File not found",
                UiLang::Ru => "Файл не найден",
            });
        }
        Ok(path)
    }
}

// ── Main entry ────────────────────────────────────────────────────────────────

pub fn run_tui(mut input: PathBuf) -> Result<()> {
    let mut selected = 0usize;
    let mut lang = UiLang::Ru;
    let mut last_message: Option<String> = None;
    let items = ui_items();

    if !input.exists() {
        input = first_run_setup(lang)?;
    }

    // Ping cache: refresh at most once every 10 seconds
    let mut cached_ping: Option<u64> = None;
    let mut last_ping_time = Instant::now()
        .checked_sub(Duration::from_secs(11))
        .unwrap_or_else(Instant::now);

    let _raw = RawModeGuard::new()?;

    loop {
        // Refresh ping cache if stale
        if last_ping_time.elapsed() >= Duration::from_secs(10) {
            cached_ping = ping_global_ms(&input);
            last_ping_time = Instant::now();
        }

        draw_tui(&input, selected, lang, &items, last_message.as_deref(), cached_ping)?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Up => selected = selected.saturating_sub(1),
                KeyCode::Down => {
                    if selected + 1 < items.len() { selected += 1; }
                }
                KeyCode::Home => selected = 0,
                KeyCode::End => selected = items.len().saturating_sub(1),
                KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
                KeyCode::Char('j') => {
                    if selected + 1 < items.len() { selected += 1; }
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
                    let result = run_tui_action(action, &mut input, lang);
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

// ── Draw (same as cli.rs draw_tui + status bar) ───────────────────────────────

fn draw_tui(
    input: &PathBuf,
    selected: usize,
    lang: UiLang,
    items: &[UiItem],
    last_message: Option<&str>,
    cached_ping: Option<u64>,
) -> Result<()> {
    let mut out = io::stdout();
    let (_, height) = terminal::size().unwrap_or((100, 30));
    let mut row: u16 = 0;

    execute!(out, cursor::MoveTo(0, 0))?;

    // Title
    draw_line(&mut out, row, "SmartRoute", Color::Cyan, true)?;
    row += 1;
    draw_line(&mut out, row, "Smart proxy router", Color::White, false)?;
    row += 1;
    draw_line(&mut out, row, "────────────────────────────────────────────────────────", Color::DarkGrey, false)?;
    row += 1;

    // Status: running
    let running = singbox_running();
    let status_text = match (lang, running) {
        (UiLang::En, true)  => "SmartRoute status: Running",
        (UiLang::En, false) => "SmartRoute status: Stopped",
        (UiLang::Ru, true)  => "Статус SmartRoute: Запущен",
        (UiLang::Ru, false) => "Статус SmartRoute: Выключен",
    };
    draw_line(&mut out, row, status_text, if running { Color::Green } else { Color::DarkGrey }, false)?;
    row += 1;

    // Status: kill-switch + ping
    let ks = killswitch_active();
    let ks_str = if ks {
        match lang { UiLang::En => "Kill-switch: ON", UiLang::Ru => "Kill-switch: ВКЛ" }
    } else {
        match lang { UiLang::En => "Kill-switch: OFF", UiLang::Ru => "Kill-switch: ВЫКЛ" }
    };
    let ping_str = match cached_ping {
        Some(ms) => format!("{}ms", ms),
        None => match lang { UiLang::En => "ping: —".to_string(), UiLang::Ru => "пинг: —".to_string() },
    };
    draw_line(&mut out, row, &format!("{}   {}", ks_str, ping_str),
        if ks { Color::Green } else { Color::DarkYellow }, false)?;
    row += 1;

    // Config path + SOCKS
    draw_line(&mut out, row, &format!("{} {}", match lang { UiLang::En => "Config:", UiLang::Ru => "Конфиг:" }, input.display()), Color::Grey, false)?;
    row += 1;
    draw_line(&mut out, row, "SOCKS5 default: 127.0.0.1:1081", Color::Grey, false)?;
    row += 1;
    draw_line(&mut out, row, match lang {
        UiLang::En => "Keys: ↑/↓ or k/j = move, Enter = run, q/Esc = exit",
        UiLang::Ru => "Клавиши: ↑/↓ или k/j = выбор, Enter = выполнить, q/Esc = выход",
    }, Color::Grey, false)?;
    row += 2;

    // Menu
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

    // Footer: hint + last message
    let footer_row = height.saturating_sub(4);
    draw_line(&mut out, footer_row, "────────────────────────────────────────────────────────", Color::DarkGrey, false)?;
    let hint = match lang { UiLang::En => items[selected].en_hint, UiLang::Ru => items[selected].ru_hint };
    draw_line(&mut out, footer_row + 1, &format!("{} {}", match lang { UiLang::En => "Hint:", UiLang::Ru => "Подсказка:" }, hint), Color::Cyan, false)?;
    draw_line(&mut out, footer_row + 2, "", Color::Green, false)?;
    if let Some(message) = last_message {
        draw_line(&mut out, footer_row + 2, message, Color::Green, false)?;
    }

    out.flush()?;
    Ok(())
}

fn draw_line(out: &mut io::Stdout, row: u16, text: &str, color: Color, bold: bool) -> Result<()> {
    execute!(out, cursor::MoveTo(0, row), terminal::Clear(ClearType::CurrentLine))?;
    if bold { execute!(out, SetAttribute(Attribute::Bold))?; }
    execute!(out, SetForegroundColor(color), Print(text), ResetColor, SetAttribute(Attribute::Reset))?;
    Ok(())
}

// ── Action handler ────────────────────────────────────────────────────────────

fn run_tui_action(action: UiAction, input: &mut PathBuf, lang: UiLang) -> Result<Option<String>> {
    clear_for_command()?;

    match action {
        UiAction::StartOnce => {
            start_smartroute(input)?;
            pause(lang)?;
            Ok(Some(match lang {
                UiLang::En => "SmartRoute started.".to_string(),
                UiLang::Ru => "SmartRoute запущен.".to_string(),
            }))
        }

        UiAction::StartDaemonSafe => {
            println!("{}", match lang {
                UiLang::En => "Starting daemon in safe mode. Press Ctrl+C to stop.",
                UiLang::Ru => "Запуск daemon в безопасном режиме. Ctrl+C чтобы остановить.",
            });
            run_daemon(input, 2, vec![], 0, 30, 8, 4, 2, 50, false)?;
            Ok(None)
        }

        UiAction::StartDaemonFull => {
            println!("{}", match lang {
                UiLang::En => "Starting daemon full preset. Press Ctrl+C to stop.",
                UiLang::Ru => "Запуск daemon с полным пресетом. Ctrl+C чтобы остановить.",
            });
            let domains = vec!["chatgpt.com".into(), "discord.com".into(), "youtube.com".into()];
            run_daemon(input, 2, domains, 300, 30, 8, 12, 3, 25, true)?;
            Ok(None)
        }

        UiAction::Stop => {
            stop_smartroute()?;
            pause(lang)?;
            Ok(Some(match lang {
                UiLang::En => "SmartRoute stopped.".to_string(),
                UiLang::Ru => "SmartRoute остановлен.".to_string(),
            }))
        }

        UiAction::ListServers => {
            let config = load_config(input)?;
            println!("{}", match lang {
                UiLang::En => "Servers:",
                UiLang::Ru => "Серверы:",
            });
            println!();
            for node in &config.nodes {
                let addr = format!("{}:{}", node.server, node.port);
                let ping = match addr.to_socket_addrs().ok().and_then(|mut a| a.next()) {
                    Some(sa) => {
                        let t = Instant::now();
                        match TcpStream::connect_timeout(&sa, Duration::from_secs(3)) {
                            Ok(_) => format!("{}ms", t.elapsed().as_millis()),
                            Err(_) => match lang { UiLang::En => "unreachable".into(), UiLang::Ru => "недоступен".into() },
                        }
                    }
                    None => "?".into(),
                };
                println!("  {:35}  {:12}  {}", node.tag, node.node_type, ping);
            }
            if !config.chains.is_empty() {
                println!();
                println!("{}", match lang { UiLang::En => "Chains:", UiLang::Ru => "Chains:" });
                for c in &config.chains {
                    println!("  {:35}  {}", c.tag, c.outbounds.join(" → "));
                }
            }
            println!();
            println!("{}: {}", match lang { UiLang::En => "Global (final)", UiLang::Ru => "Глобальный (final)" }, config.general.final_outbound);
            pause(lang)?;
            Ok(None)
        }

        UiAction::SetGlobal => {
            let mut config = load_config(input)?;
            println!("{}", match lang {
                UiLang::En => "Available servers and chains:",
                UiLang::Ru => "Доступные серверы и chains:",
            });
            println!("  direct");
            for n in &config.nodes { println!("  {}", n.tag); }
            for c in &config.chains { println!("  {} (chain)", c.tag); }
            println!();
            println!("{}: {}", match lang { UiLang::En => "Current global", UiLang::Ru => "Текущий глобальный" }, config.general.final_outbound);
            println!();
            let tag = prompt_line(match lang {
                UiLang::En => "New global server/chain: ",
                UiLang::Ru => "Новый глобальный сервер/chain: ",
            })?;
            if tag.is_empty() {
                pause(lang)?;
                return Ok(Some(match lang { UiLang::En => "Cancelled.".into(), UiLang::Ru => "Отменено.".into() }));
            }
            config.general.final_outbound = tag.clone();
            validate_config(&config)?;
            crate::config::save_config(input, &config)?;
            pause(lang)?;
            Ok(Some(format!("{}: {}", match lang { UiLang::En => "Global set", UiLang::Ru => "Глобальный установлен" }, tag)))
        }

        UiAction::KillSwitchEnable => {
            enable_killswitch(input, true)?;
            pause(lang)?;
            Ok(Some(match lang {
                UiLang::En => "Kill-switch enabled.".to_string(),
                UiLang::Ru => "Kill-switch включён.".to_string(),
            }))
        }

        UiAction::KillSwitchDisable => {
            disable_killswitch()?;
            pause(lang)?;
            Ok(Some(match lang {
                UiLang::En => "Kill-switch disabled.".to_string(),
                UiLang::Ru => "Kill-switch выключен.".to_string(),
            }))
        }

        UiAction::SiteRule => {
            let mut config = load_config(input)?;
            println!("{}", match lang { UiLang::En => "Available servers and chains:", UiLang::Ru => "Доступные серверы и chains:" });
            for n in &config.nodes { println!("  {}", n.tag); }
            for c in &config.chains { println!("  {} (chain)", c.tag); }
            println!();
            let domain = prompt_line(match lang {
                UiLang::En => "Domain suffix (e.g. youtube.com): ",
                UiLang::Ru => "Доменный суффикс (например youtube.com): ",
            })?;
            if domain.is_empty() {
                pause(lang)?;
                return Ok(Some(match lang { UiLang::En => "Cancelled.".into(), UiLang::Ru => "Отменено.".into() }));
            }
            let outbound = prompt_line(match lang {
                UiLang::En => "Server/chain tag: ",
                UiLang::Ru => "Тег сервера/chain: ",
            })?;
            if outbound.is_empty() {
                pause(lang)?;
                return Ok(Some(match lang { UiLang::En => "Cancelled.".into(), UiLang::Ru => "Отменено.".into() }));
            }
            config.rules.retain(|r| !(r.rule_type == "domain_suffix" && r.value == domain));
            config.rules.push(Rule { rule_type: "domain_suffix".into(), value: domain.clone(), outbound: outbound.clone() });
            validate_config(&config)?;
            write_config_toml(input, &config)?;
            pause(lang)?;
            Ok(Some(format!("{} → {}", domain, outbound)))
        }

        UiAction::AppRule => {
            let mut config = load_config(input)?;
            println!("{}", match lang {
                UiLang::En => "Creates a separate local SOCKS5 port for an app.",
                UiLang::Ru => "Создаёт отдельный локальный SOCKS5-порт для приложения.",
            });
            println!("{}", match lang {
                UiLang::En => "Example: Zen uses 127.0.0.1:1082, Telegram uses 127.0.0.1:1083.",
                UiLang::Ru => "Пример: Zen использует 127.0.0.1:1082, Telegram — 127.0.0.1:1083.",
            });
            println!();
            println!("{}", match lang { UiLang::En => "Available servers and chains:", UiLang::Ru => "Доступные серверы и chains:" });
            for n in &config.nodes { println!("  {}", n.tag); }
            for c in &config.chains { println!("  {} (chain)", c.tag); }
            println!();
            let tag = prompt_line(match lang { UiLang::En => "Profile name (e.g. telegram): ", UiLang::Ru => "Название профиля (например telegram): " })?;
            if tag.is_empty() {
                pause(lang)?;
                return Ok(Some(match lang { UiLang::En => "Cancelled.".into(), UiLang::Ru => "Отменено.".into() }));
            }
            let port_str = prompt_line(match lang { UiLang::En => "Local SOCKS port (e.g. 1082): ", UiLang::Ru => "Локальный SOCKS-порт (например 1082): " })?;
            let listen_port: u16 = port_str.parse().context(match lang { UiLang::En => "Invalid port", UiLang::Ru => "Неверный порт" })?;
            let outbound = prompt_line(match lang { UiLang::En => "Server/chain tag: ", UiLang::Ru => "Тег сервера/chain: " })?;
            if outbound.is_empty() {
                pause(lang)?;
                return Ok(Some(match lang { UiLang::En => "Cancelled.".into(), UiLang::Ru => "Отменено.".into() }));
            }
            config.local_profiles.retain(|p| p.tag != tag);
            config.local_profiles.push(LocalProfile { tag: tag.clone(), listen: "127.0.0.1".into(), listen_port, outbound: outbound.clone() });
            validate_config(&config)?;
            write_config_toml(input, &config)?;
            pause(lang)?;
            Ok(Some(format!("{}: 127.0.0.1:{} → {}", tag, listen_port, outbound)))
        }

        UiAction::ShowRules => {
            let config = load_config(input)?;
            println!("{}", match lang { UiLang::En => "Routing rules:", UiLang::Ru => "Правила маршрутизации:" });
            println!();
            if config.rules.is_empty() {
                println!("  {}", match lang { UiLang::En => "(no rules)", UiLang::Ru => "(нет правил)" });
            } else {
                for (i, r) in config.rules.iter().enumerate() {
                    println!("  [{:2}] {:14} {:35} → {}", i, r.rule_type, r.value, r.outbound);
                }
            }
            println!();
            println!("{}", match lang { UiLang::En => "Chains:", UiLang::Ru => "Chains:" });
            if config.chains.is_empty() {
                println!("  {}", match lang { UiLang::En => "(no chains)", UiLang::Ru => "(нет chains)" });
            } else {
                for c in &config.chains {
                    println!("  {:25} = {}", c.tag, c.outbounds.join(" → "));
                }
            }
            println!();
            println!("{}: {}", match lang { UiLang::En => "Global (final)", UiLang::Ru => "Глобальный (final)" }, config.general.final_outbound);
            pause(lang)?;
            Ok(None)
        }

        UiAction::DeleteRule => {
            let mut config = load_config(input)?;
            // Show rules first
            if config.rules.is_empty() {
                println!("{}", match lang { UiLang::En => "(no rules)", UiLang::Ru => "(нет правил)" });
            } else {
                for (i, r) in config.rules.iter().enumerate() {
                    println!("  [{:2}] {:14} {:35} → {}", i, r.rule_type, r.value, r.outbound);
                }
            }
            if !config.chains.is_empty() {
                println!();
                for c in &config.chains { println!("  chain {:25} = {}", c.tag, c.outbounds.join(" → ")); }
            }
            println!();
            println!("{}", match lang {
                UiLang::En => "Enter rule number to delete, or 'chain <tag>' to delete a chain:",
                UiLang::Ru => "Введи номер правила для удаления или 'chain <тег>' для удаления chain:",
            });
            let inp = prompt_line("> ")?;
            if inp.is_empty() {
                pause(lang)?;
                return Ok(Some(match lang { UiLang::En => "Cancelled.".into(), UiLang::Ru => "Отменено.".into() }));
            }
            let msg = if let Some(chain_tag) = inp.strip_prefix("chain ") {
                let before = config.chains.len();
                config.chains.retain(|c| c.tag != chain_tag.trim());
                if config.chains.len() == before {
                    format!("{}: {}", match lang { UiLang::En => "Chain not found", UiLang::Ru => "Chain не найден" }, chain_tag)
                } else {
                    config.rules.retain(|r| r.outbound != chain_tag.trim());
                    write_config_toml(input, &config)?;
                    format!("{}: {}", match lang { UiLang::En => "Chain deleted", UiLang::Ru => "Chain удалён" }, chain_tag)
                }
            } else {
                let idx: usize = inp.parse().context(match lang { UiLang::En => "Invalid number", UiLang::Ru => "Неверный номер" })?;
                if idx >= config.rules.len() {
                    match lang { UiLang::En => "Index out of range.".into(), UiLang::Ru => "Номер вне диапазона.".into() }
                } else {
                    let removed = config.rules.remove(idx);
                    write_config_toml(input, &config)?;
                    format!("{}: {} {} → {}", match lang { UiLang::En => "Deleted", UiLang::Ru => "Удалено" }, removed.rule_type, removed.value, removed.outbound)
                }
            };
            pause(lang)?;
            Ok(Some(msg))
        }

        UiAction::LeakTest => {
            run_leak_test(input, "google.com", None)?;
            pause(lang)?;
            Ok(None)
        }

        UiAction::KillSwitchMenu => {
            let active = killswitch_active();
            println!("{}: {}", match lang { UiLang::En => "Kill-switch now", UiLang::Ru => "Kill-switch сейчас" },
                if active { match lang { UiLang::En => "ON", UiLang::Ru => "ВКЛЮЧЁН" } }
                else      { match lang { UiLang::En => "OFF", UiLang::Ru => "ВЫКЛЮЧЕН" } });
            println!();
            println!("{}", match lang {
                UiLang::En => "1 — Enable   2 — Disable   Enter — cancel",
                UiLang::Ru => "1 — Включить   2 — Выключить   Enter — отмена",
            });
            let choice = prompt_line("> ")?;
            let msg = match choice.as_str() {
                "1" => { enable_killswitch(input, true)?; match lang { UiLang::En => "Kill-switch enabled.", UiLang::Ru => "Kill-switch включён." }.to_string() }
                "2" => { disable_killswitch()?; match lang { UiLang::En => "Kill-switch disabled.", UiLang::Ru => "Kill-switch выключен." }.to_string() }
                _   => match lang { UiLang::En => "Cancelled.", UiLang::Ru => "Отменено." }.to_string(),
            };
            pause(lang)?;
            Ok(Some(msg))
        }

        UiAction::ToggleLanguage | UiAction::Exit => Ok(None),
    }
}

// ── Helpers (identical to cli.rs) ────────────────────────────────────────────

fn clear_for_command() -> Result<()> {
    execute!(io::stdout(), terminal::Clear(ClearType::Purge), terminal::Clear(ClearType::All), cursor::MoveTo(0, 0))?;
    Ok(())
}

fn prompt_line(prompt: &str) -> Result<String> {
    print!("{}", prompt);
    io::stdout().flush()?;
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

fn pause(lang: UiLang) -> Result<()> {
    println!();
    println!("{}", match lang {
        UiLang::En => "Press Enter to return to menu...",
        UiLang::Ru => "Нажми Enter, чтобы вернуться в меню...",
    });
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    clear_for_command()?;
    Ok(())
}
