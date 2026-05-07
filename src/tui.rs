use crate::{
    config::{LocalProfile, Rule, load_config, save_config, validate_config},
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
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, Instant},
};

// ── Language ──────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Ru,
    En,
}

macro_rules! t {
    ($lang:expr, $ru:expr, $en:expr) => {
        match $lang {
            Lang::Ru => $ru,
            Lang::En => $en,
        }
    };
}

// ── Menu items ────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
enum Item {
    Start,
    DaemonSafe,
    DaemonFull,
    Stop,
    ListServers,
    SetGlobal,
    MaskOn,
    MaskOff,
    SiteRule,
    AppRule,
    ShowRules,
    DeleteRule,
    LeakTest,
    Language,
    KillSwitch,
    Exit,
}

struct MenuItem {
    item: Item,
    ru: &'static str,
    en: &'static str,
}

fn menu() -> Vec<MenuItem> {
    vec![
        MenuItem { item: Item::Start,      ru: "1. Запуск",                                    en: "1. Start" },
        MenuItem { item: Item::DaemonSafe, ru: "2. Daemon — безопасный режим",                 en: "2. Daemon — safe mode" },
        MenuItem { item: Item::DaemonFull, ru: "3. Daemon — полный пресет",                    en: "3. Daemon — full preset" },
        MenuItem { item: Item::Stop,       ru: "4. Остановить",                                en: "4. Stop" },
        MenuItem { item: Item::ListServers,ru: "5. Список серверов (имя, протокол, пинг)",     en: "5. Server list (name, protocol, ping)" },
        MenuItem { item: Item::SetGlobal,  ru: "6. Выбрать глобальный сервер / chain",         en: "6. Set global server / chain" },
        MenuItem { item: Item::MaskOn,     ru: "7. Включить маскировку трафика",               en: "7. Enable traffic masking" },
        MenuItem { item: Item::MaskOff,    ru: "8. Выключить маскировку трафика",              en: "8. Disable traffic masking" },
        MenuItem { item: Item::SiteRule,   ru: "9. Выбрать сервер / chain для сайта",          en: "9. Set server / chain for site" },
        MenuItem { item: Item::AppRule,    ru: "10. Выбрать сервер / chain для приложения",    en: "10. Set server / chain for app" },
        MenuItem { item: Item::ShowRules,  ru: "11. Показать все правила и chains",            en: "11. Show all rules and chains" },
        MenuItem { item: Item::DeleteRule, ru: "12. Удалить правило / chain",                  en: "12. Delete rule / chain" },
        MenuItem { item: Item::LeakTest,   ru: "13. Проверка на утечки",                       en: "13. Leak test" },
        MenuItem { item: Item::Language,   ru: "14. Язык: Русский / English",                  en: "14. Language: English / Русский" },
        MenuItem { item: Item::KillSwitch, ru: "15. Kill-switch",                              en: "15. Kill-switch" },
        MenuItem { item: Item::Exit,       ru: "16. Выход",                                    en: "16. Exit" },
    ]
}

// ── Raw mode guard ────────────────────────────────────────────────────────────

struct RawModeGuard;

impl RawModeGuard {
    fn new() -> Result<Self> {
        terminal::enable_raw_mode()?;
        execute!(
            io::stdout(),
            terminal::EnterAlternateScreen,
            cursor::Hide,
            terminal::Clear(ClearType::All)
        )?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = execute!(
            io::stdout(),
            cursor::Show,
            terminal::LeaveAlternateScreen
        );
        let _ = terminal::disable_raw_mode();
    }
}

// ── First run ─────────────────────────────────────────────────────────────────

fn first_run_setup(lang: Lang) -> Result<PathBuf> {
    println!("{}", t!(lang, "Первый запуск SmartRoute", "SmartRoute first run"));
    println!();
    println!("{}", t!(lang, 
        "Введите путь к конфигу (например, imported.toml) или subscription URL:",
        "Enter config path (e.g., imported.toml) or subscription URL:"
    ));
    print!("> ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();

    if input.is_empty() {
        anyhow::bail!(t!(lang, "Отменено: пустой ввод", "Cancelled: empty input"));
    }

    if input.starts_with("http://") || input.starts_with("https://") {
        let output = PathBuf::from("imported.toml");
        println!("{}", t!(lang, "Импорт подписки...", "Importing subscription..."));
        import_url(input, &output)?;
        println!("{}", t!(lang, "Готово. Конфиг сохранён в imported.toml", "Done. Config saved to imported.toml"));
        Ok(output)
    } else {
        let path = PathBuf::from(input);
        if !path.exists() {
            anyhow::bail!(t!(lang, "Файл не найден", "File not found"));
        }
        Ok(path)
    }
}

// ── Main entry ────────────────────────────────────────────────────────────────

pub fn run_tui(mut config_path: PathBuf) -> Result<()> {
    let mut lang = Lang::Ru;
    let mut selected = 0usize;
    let mut message: Option<String> = None;

    // First run check
    if !config_path.exists() {
        config_path = first_run_setup(lang)?;
    }

    let _raw = RawModeGuard::new()?;
    let items = menu();

    loop {
        draw_ui(&config_path, selected, lang, &items, message.as_deref())?;

        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => selected = selected.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => {
                    if selected + 1 < items.len() {
                        selected += 1;
                    }
                }
                KeyCode::Home => selected = 0,
                KeyCode::End => selected = items.len().saturating_sub(1),
                KeyCode::Esc | KeyCode::Char('q') => return Ok(()),
                KeyCode::Enter => {
                    let action = items[selected].item;

                    if action == Item::Exit {
                        return Ok(());
                    }

                    if action == Item::Language {
                        lang = match lang {
                            Lang::Ru => Lang::En,
                            Lang::En => Lang::Ru,
                        };
                        message = Some(t!(lang, "Язык изменён", "Language changed").to_string());
                        continue;
                    }

                    terminal::disable_raw_mode()?;
                    let result = handle_action(action, &mut config_path, lang);
                    terminal::enable_raw_mode()?;

                    match result {
                        Ok(msg) => message = msg,
                        Err(e) => message = Some(format!("ERROR: {:#}", e)),
                    }
                }
                _ => {}
            }
        }
    }
}

// ── Status helpers ────────────────────────────────────────────────────────────

fn is_running() -> bool {
    Command::new("pgrep")
        .args(["-x", "sing-box"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn killswitch_active() -> bool {
    Command::new("nft")
        .args(["list", "table", "inet", "smartroute"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Ping the global outbound node by TCP connect to its server:port.
/// Returns latency in ms or None.
fn ping_global(config_path: &Path) -> Option<u64> {
    let config = load_config(config_path).ok()?;
    let final_tag = &config.general.final_outbound;

    // Find the node for final_outbound (direct chain resolution not needed for ping)
    let node = config.nodes.iter().find(|n| &n.tag == final_tag).or_else(|| {
        // If final is a chain, find the last hop
        config.chains.iter()
            .find(|c| &c.tag == final_tag)
            .and_then(|chain| chain.outbounds.last())
            .and_then(|last| config.nodes.iter().find(|n| &n.tag == last))
    })?;

    let addr = format!("{}:{}", node.server, node.port);
    let socket_addr = addr.to_socket_addrs().ok()?.next()?;

    let start = Instant::now();
    TcpStream::connect_timeout(&socket_addr, Duration::from_secs(3)).ok()?;
    Some(start.elapsed().as_millis() as u64)
}

// ── Draw ──────────────────────────────────────────────────────────────────────

fn draw_ui(
    config_path: &Path,
    selected: usize,
    lang: Lang,
    items: &[MenuItem],
    message: Option<&str>,
) -> Result<()> {
    let mut out = io::stdout();
    let (_, height) = terminal::size().unwrap_or((120, 30));

    execute!(out, cursor::MoveTo(0, 0), terminal::Clear(ClearType::All))?;

    let running = is_running();
    let ks = killswitch_active();
    let ping = ping_global(config_path);

    // ── Header ──
    let status_str = if running {
        t!(lang, "● Запущен", "● Running")
    } else {
        t!(lang, "○ Выключен", "○ Stopped")
    };
    let ks_str = if ks {
        t!(lang, "Kill-switch: ON", "Kill-switch: ON")
    } else {
        t!(lang, "Kill-switch: OFF", "Kill-switch: OFF")
    };
    let ping_str = match ping {
        Some(ms) => format!("{}ms", ms),
        None => t!(lang, "пинг: —", "ping: —").to_string(),
    };

    let header = format!("SmartRoute  {}  {}  {}", status_str, ks_str, ping_str);
    put_line(&mut out, 0, &header, Color::Cyan, true)?;
    put_line(&mut out, 1, &format!("{}: {}", t!(lang, "Конфиг", "Config"), config_path.display()), Color::DarkGrey, false)?;
    put_line(&mut out, 2, "─────────────────────────────────────────────────────────────", Color::DarkGrey, false)?;

    // ── Menu ──
    let menu_start: u16 = 3;
    let footer_rows: u16 = 3;
    let max_visible = (height.saturating_sub(menu_start + footer_rows)) as usize;
    let visible = items.len().min(max_visible.max(1));
    let offset = if selected >= visible { selected + 1 - visible } else { 0 };

    for (idx, item) in items.iter().enumerate().skip(offset).take(visible) {
        let label = t!(lang, item.ru, item.en);
        let row = menu_start + (idx - offset) as u16;
        if idx == selected {
            put_line(&mut out, row, &format!("▶ {}", label), Color::Yellow, true)?;
        } else {
            put_line(&mut out, row, &format!("  {}", label), Color::White, false)?;
        }
    }

    // ── Footer ──
    let footer = height.saturating_sub(footer_rows);
    put_line(&mut out, footer, "─────────────────────────────────────────────────────────────", Color::DarkGrey, false)?;
    put_line(&mut out, footer + 1, t!(lang,
        "↑↓/jk — выбор   Enter — выполнить   q — выход",
        "↑↓/jk — move   Enter — run   q — quit"
    ), Color::DarkGrey, false)?;

    if let Some(msg) = message {
        let color = if msg.starts_with("ERROR") { Color::Red } else { Color::Green };
        put_line(&mut out, footer + 2, msg, color, false)?;
    }

    out.flush()?;
    Ok(())
}

fn put_line(out: &mut io::Stdout, row: u16, text: &str, color: Color, bold: bool) -> Result<()> {
    execute!(out, cursor::MoveTo(0, row), terminal::Clear(ClearType::CurrentLine))?;
    if bold { execute!(out, SetAttribute(Attribute::Bold))?; }
    execute!(out, SetForegroundColor(color), Print(text), ResetColor, SetAttribute(Attribute::Reset))?;
    Ok(())
}

// ── Action handler ────────────────────────────────────────────────────────────

fn handle_action(action: Item, config_path: &mut PathBuf, lang: Lang) -> Result<Option<String>> {
    clear_screen()?;

    match action {
        Item::Start => {
            start_smartroute(config_path)?;
            pause(lang)?;
            Ok(Some(t!(lang, "SmartRoute запущен.", "SmartRoute started.").to_string()))
        }

        Item::DaemonSafe => {
            println!("{}", t!(lang, "Daemon (безопасный режим). Ctrl+C для остановки.", "Daemon (safe mode). Ctrl+C to stop."));
            run_daemon(config_path, 2, vec![], 0, 30, 8, 4, 2, 50, false)?;
            Ok(None)
        }

        Item::DaemonFull => {
            println!("{}", t!(lang, "Daemon (полный пресет). Ctrl+C для остановки.", "Daemon (full preset). Ctrl+C to stop."));
            let domains = vec!["youtube.com".into(), "discord.com".into(), "chatgpt.com".into()];
            run_daemon(config_path, 2, domains, 300, 30, 8, 12, 3, 25, true)?;
            Ok(None)
        }

        Item::Stop => {
            stop_smartroute()?;
            pause(lang)?;
            Ok(Some(t!(lang, "SmartRoute остановлен.", "SmartRoute stopped.").to_string()))
        }

        Item::ListServers => {
            action_list_servers(config_path, lang)?;
            pause(lang)?;
            Ok(None)
        }

        Item::SetGlobal => {
            let msg = action_set_global(config_path, lang)?;
            pause(lang)?;
            Ok(Some(msg))
        }

        Item::MaskOn => {
            enable_killswitch(config_path, true)?;
            pause(lang)?;
            Ok(Some(t!(lang, "Kill-switch включён.", "Kill-switch enabled.").to_string()))
        }

        Item::MaskOff => {
            disable_killswitch()?;
            pause(lang)?;
            Ok(Some(t!(lang, "Kill-switch выключен.", "Kill-switch disabled.").to_string()))
        }

        Item::SiteRule => {
            let msg = action_site_rule(config_path, lang)?;
            pause(lang)?;
            Ok(Some(msg))
        }

        Item::AppRule => {
            let msg = action_app_rule(config_path, lang)?;
            pause(lang)?;
            Ok(Some(msg))
        }

        Item::ShowRules => {
            action_show_rules(config_path, lang)?;
            pause(lang)?;
            Ok(None)
        }

        Item::DeleteRule => {
            let msg = action_delete_rule(config_path, lang)?;
            pause(lang)?;
            Ok(Some(msg))
        }

        Item::LeakTest => {
            run_leak_test(config_path, "google.com", None)?;
            pause(lang)?;
            Ok(None)
        }

        Item::KillSwitch => {
            action_killswitch(config_path, lang)?;
            pause(lang)?;
            Ok(None)
        }

        Item::Language | Item::Exit => Ok(None),
    }
}

// ── Individual actions ────────────────────────────────────────────────────────

fn action_list_servers(config_path: &Path, lang: Lang) -> Result<()> {
    let config = load_config(config_path)?;

    println!("{}", t!(lang, "Серверы:", "Servers:"));
    println!();

    for node in &config.nodes {
        let addr = format!("{}:{}", node.server, node.port);
        let ping = match addr.to_socket_addrs().ok().and_then(|mut a| a.next()) {
            Some(sa) => {
                let start = Instant::now();
                match TcpStream::connect_timeout(&sa, Duration::from_secs(3)) {
                    Ok(_) => format!("{}ms", start.elapsed().as_millis()),
                    Err(_) => t!(lang, "недоступен", "unreachable").to_string(),
                }
            }
            None => "?".to_string(),
        };
        println!("  {:30}  {:12}  {}", node.tag, node.node_type, ping);
    }

    if !config.chains.is_empty() {
        println!();
        println!("{}", t!(lang, "Chains:", "Chains:"));
        for chain in &config.chains {
            println!("  {:30}  {}", chain.tag, chain.outbounds.join(" → "));
        }
    }

    println!();
    println!("{}: {}", t!(lang, "Глобальный", "Global"), config.general.final_outbound);
    Ok(())
}

fn action_set_global(config_path: &Path, lang: Lang) -> Result<String> {
    let mut config = load_config(config_path)?;

    println!("{}", t!(lang, "Доступные серверы и chains:", "Available servers and chains:"));
    println!("  direct");
    for n in &config.nodes { println!("  {}", n.tag); }
    for c in &config.chains { println!("  {} (chain)", c.tag); }
    println!();
    println!("{}: {}", t!(lang, "Текущий глобальный", "Current global"), config.general.final_outbound);
    println!();

    let tag = prompt(t!(lang, "Новый глобальный сервер/chain: ", "New global server/chain: "))?;
    if tag.is_empty() {
        return Ok(t!(lang, "Отменено.", "Cancelled.").to_string());
    }

    config.general.final_outbound = tag.clone();
    validate_config(&config)?;
    save_config(config_path, &config)?;

    Ok(format!("{}: {}", t!(lang, "Глобальный сервер установлен", "Global server set"), tag))
}

fn action_site_rule(config_path: &Path, lang: Lang) -> Result<String> {
    let mut config = load_config(config_path)?;

    println!("{}", t!(lang, "Доступные серверы и chains:", "Available servers and chains:"));
    for n in &config.nodes { println!("  {}", n.tag); }
    for c in &config.chains { println!("  {} (chain)", c.tag); }
    println!();

    let domain = prompt(t!(lang, "Домен (например youtube.com): ", "Domain (e.g. youtube.com): "))?;
    if domain.is_empty() {
        return Ok(t!(lang, "Отменено.", "Cancelled.").to_string());
    }

    let outbound = prompt(t!(lang, "Сервер/chain для этого домена: ", "Server/chain for this domain: "))?;
    if outbound.is_empty() {
        return Ok(t!(lang, "Отменено.", "Cancelled.").to_string());
    }

    config.rules.retain(|r| !(r.rule_type == "domain_suffix" && r.value == domain));
    config.rules.push(Rule { rule_type: "domain_suffix".into(), value: domain.clone(), outbound: outbound.clone() });
    validate_config(&config)?;
    write_config_toml(config_path, &config)?;

    Ok(format!("{} → {}", domain, outbound))
}

fn action_app_rule(config_path: &Path, lang: Lang) -> Result<String> {
    let mut config = load_config(config_path)?;

    println!("{}", t!(lang, "Создать отдельный SOCKS-порт для приложения.", "Create a separate SOCKS port for an app."));
    println!("{}", t!(lang, "Укажи этот порт в настройках прокси приложения.", "Set this port in the app's proxy settings."));
    println!();
    println!("{}", t!(lang, "Доступные серверы и chains:", "Available servers and chains:"));
    for n in &config.nodes { println!("  {}", n.tag); }
    for c in &config.chains { println!("  {} (chain)", c.tag); }
    println!();

    let tag = prompt(t!(lang, "Название профиля (например telegram): ", "Profile name (e.g. telegram): "))?;
    if tag.is_empty() {
        return Ok(t!(lang, "Отменено.", "Cancelled.").to_string());
    }

    let port_str = prompt(t!(lang, "Локальный порт (например 1082): ", "Local port (e.g. 1082): "))?;
    let port: u16 = port_str.parse().context(t!(lang, "Неверный порт", "Invalid port"))?;

    let outbound = prompt(t!(lang, "Сервер/chain: ", "Server/chain: "))?;
    if outbound.is_empty() {
        return Ok(t!(lang, "Отменено.", "Cancelled.").to_string());
    }

    config.local_profiles.retain(|p| p.tag != tag);
    config.local_profiles.push(LocalProfile {
        tag: tag.clone(),
        listen: "127.0.0.1".into(),
        listen_port: port,
        outbound: outbound.clone(),
    });
    validate_config(&config)?;
    write_config_toml(config_path, &config)?;

    Ok(format!("{}: 127.0.0.1:{} → {}", tag, port, outbound))
}

fn action_show_rules(config_path: &Path, lang: Lang) -> Result<()> {
    let config = load_config(config_path)?;

    println!("{}", t!(lang, "Правила маршрутизации:", "Routing rules:"));
    println!();
    if config.rules.is_empty() {
        println!("  {}", t!(lang, "(нет правил)", "(no rules)"));
    } else {
        for (i, r) in config.rules.iter().enumerate() {
            println!("  [{:2}] {:14} {:35} → {}", i, r.rule_type, r.value, r.outbound);
        }
    }

    println!();
    println!("{}", t!(lang, "Chains:", "Chains:"));
    if config.chains.is_empty() {
        println!("  {}", t!(lang, "(нет chains)", "(no chains)"));
    } else {
        for c in &config.chains {
            println!("  {:20} = {}", c.tag, c.outbounds.join(" → "));
        }
    }

    println!();
    println!("{}: {}", t!(lang, "Глобальный (final)", "Global (final)"), config.general.final_outbound);
    Ok(())
}

fn action_delete_rule(config_path: &Path, lang: Lang) -> Result<String> {
    let mut config = load_config(config_path)?;

    action_show_rules(config_path, lang)?;
    println!();
    println!("{}", t!(lang,
        "Введи номер правила для удаления, или 'chain <tag>' для удаления chain:",
        "Enter rule number to delete, or 'chain <tag>' to delete a chain:"
    ));

    let input = prompt("> ")?;
    if input.is_empty() {
        return Ok(t!(lang, "Отменено.", "Cancelled.").to_string());
    }

    if let Some(chain_tag) = input.strip_prefix("chain ") {
        let before = config.chains.len();
        config.chains.retain(|c| c.tag != chain_tag.trim());
        if config.chains.len() == before {
            return Ok(format!("{}: {}", t!(lang, "Chain не найден", "Chain not found"), chain_tag));
        }
        // Also remove rules pointing to this chain
        config.rules.retain(|r| r.outbound != chain_tag.trim());
        write_config_toml(config_path, &config)?;
        return Ok(format!("{}: {}", t!(lang, "Chain удалён", "Chain deleted"), chain_tag));
    }

    let idx: usize = input.parse().context(t!(lang, "Неверный номер", "Invalid number"))?;
    if idx >= config.rules.len() {
        return Ok(t!(lang, "Номер вне диапазона.", "Index out of range.").to_string());
    }

    let removed = config.rules.remove(idx);
    write_config_toml(config_path, &config)?;

    Ok(format!("{}: {} {} → {}", t!(lang, "Удалено", "Deleted"), removed.rule_type, removed.value, removed.outbound))
}

fn action_killswitch(config_path: &Path, lang: Lang) -> Result<()> {
    let active = killswitch_active();
    println!("{}: {}", t!(lang, "Kill-switch сейчас", "Kill-switch now"),
        if active { t!(lang, "ВКЛЮЧЁН", "ON") } else { t!(lang, "ВЫКЛЮЧЕН", "OFF") });
    println!();
    println!("{}", t!(lang,
        "1 — Включить   2 — Выключить   Enter — отмена",
        "1 — Enable   2 — Disable   Enter — cancel"
    ));

    let choice = prompt("> ")?;
    match choice.as_str() {
        "1" => { enable_killswitch(config_path, true)?; println!("{}", t!(lang, "Kill-switch включён.", "Kill-switch enabled.")); }
        "2" => { disable_killswitch()?; println!("{}", t!(lang, "Kill-switch выключен.", "Kill-switch disabled.")); }
        _ => println!("{}", t!(lang, "Отменено.", "Cancelled.")),
    }
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn prompt(label: &str) -> Result<String> {
    print!("{}", label);
    io::stdout().flush()?;
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    Ok(s.trim().to_string())
}

fn pause(lang: Lang) -> Result<()> {
    println!();
    println!("{}", t!(lang, "Нажми Enter для возврата в меню...", "Press Enter to return to menu..."));
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    Ok(())
}

fn clear_screen() -> Result<()> {
    execute!(io::stdout(), terminal::Clear(ClearType::All), cursor::MoveTo(0, 0))?;
    Ok(())
}
