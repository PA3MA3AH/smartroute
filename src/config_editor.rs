use std::io::{self, Write};
use std::path::Path;

use crate::config::{load_config, save_config};
use crate::runtime::{start_smartroute, stop_smartroute};

fn read_choice() -> Option<usize> {
    print!("> ");
    let _ = io::stdout().flush();

    let mut input = String::new();
    io::stdin().read_line(&mut input).ok()?;
    input.trim().parse::<usize>().ok()
}

fn pause() {
    println!();
    println!("Нажмите Enter, чтобы вернуться...");
    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input);
}

fn collect_outbounds(config: &crate::config::SmartRouteConfig) -> Vec<String> {
    let mut out = Vec::new();

    out.push("direct".to_string());
    out.push("block".to_string());

    for node in &config.nodes {
        out.push(node.tag.clone());
    }

    for chain in &config.chains {
        out.push(chain.tag.clone());
    }

    out
}

pub fn edit_sites_config(path: &Path) {
    let mut config = match load_config(path) {
        Ok(cfg) => cfg,
        Err(e) => {
            println!("Ошибка загрузки конфига: {e}");
            pause();
            return;
        }
    };

    loop {
        println!();
        println!("Выберите домен сайта:");
        println!("────────────────────────────────────────────────────────");

        for (i, rule) in config.rules.iter().enumerate() {
            println!(
                "[{}] {} {} -> {} | ping: -",
                i, rule.rule_type, rule.value, rule.outbound
            );
        }

        println!("[{}] Выход", config.rules.len());

        let Some(choice) = read_choice() else {
            continue;
        };

        if choice == config.rules.len() {
            return;
        }

        if choice >= config.rules.len() {
            println!("Некорректный выбор");
            continue;
        }

        let old_value = config.rules[choice].value.clone();
        let old_outbound = config.rules[choice].outbound.clone();

        println!();
        println!("Вы выбрали: {}", old_value);
        println!("Текущий прокси: {}", old_outbound);
        println!();
        println!("Какой прокси выбрать:");
        println!("────────────────────────────────────────────────────────");

        let outbounds = collect_outbounds(&config);

        for (i, outbound) in outbounds.iter().enumerate() {
            println!("[{}] {} | ping: -", i, outbound);
        }

        println!("[{}] Выход", outbounds.len());

        let Some(proxy_choice) = read_choice() else {
            continue;
        };

        if proxy_choice == outbounds.len() {
            continue;
        }

        if proxy_choice >= outbounds.len() {
            println!("Некорректный выбор");
            continue;
        }

        let new_outbound = outbounds[proxy_choice].clone();

        println!();
        println!("Вы выбрали: {} для домена {}", new_outbound, old_value);
        println!("Вы хотите применить изменения?");
        println!("[0] Да");
        println!("[1] Нет");
        println!("[2] Выход");

        let confirm = loop {
            if let Some(c) = read_choice() {
                break c;
            }
            println!("Введите число 0/1/2");
        };

        if confirm != 0 {
            println!("Изменения отменены.");
            pause();
            continue;
        }

        config.rules[choice].outbound = new_outbound.clone();

        println!();
        println!(
            "Применяем обновление правил для {} на {}...",
            old_value, new_outbound
        );

        if let Err(e) = save_config(path, &config) {
            println!("Ошибка сохранения конфига: {e}");
            pause();
            continue;
        }

        let _ = stop_smartroute();
        match start_smartroute(path) {
            Ok(_) => println!("Успешно"),
            Err(e) => println!("Конфиг сохранён, но SmartRoute не запустился: {e}"),
        }

        pause();
    }
}

pub fn edit_apps_config(path: &Path) {
    let mut config = match load_config(path) {
        Ok(cfg) => cfg,
        Err(e) => {
            println!("Ошибка загрузки конфига: {e}");
            pause();
            return;
        }
    };

    loop {
        println!();
        println!("Выберите приложение / локальный порт:");
        println!("────────────────────────────────────────────────────────");

        for (i, profile) in config.local_profiles.iter().enumerate() {
            println!(
                "[{}] {} {}:{} -> {} | ping: -",
                i, profile.tag, profile.listen, profile.listen_port, profile.outbound
            );
        }

        println!("[{}] Выход", config.local_profiles.len());

        let Some(choice) = read_choice() else {
            continue;
        };

        if choice == config.local_profiles.len() {
            return;
        }

        if choice >= config.local_profiles.len() {
            println!("Некорректный выбор");
            continue;
        }

        let old_tag = config.local_profiles[choice].tag.clone();
        let old_outbound = config.local_profiles[choice].outbound.clone();

        println!();
        println!("Вы выбрали: {}", old_tag);
        println!("Текущий прокси: {}", old_outbound);
        println!();
        println!("Какой прокси выбрать:");
        println!("────────────────────────────────────────────────────────");

        let outbounds = collect_outbounds(&config);

        for (i, outbound) in outbounds.iter().enumerate() {
            println!("[{}] {} | ping: -", i, outbound);
        }

        println!("[{}] Выход", outbounds.len());

        let Some(proxy_choice) = read_choice() else {
            continue;
        };

        if proxy_choice == outbounds.len() {
            continue;
        }

        if proxy_choice >= outbounds.len() {
            println!("Некорректный выбор");
            continue;
        }

        let new_outbound = outbounds[proxy_choice].clone();

        println!();
        println!("Вы выбрали: {} для приложения {}", new_outbound, old_tag);
        println!("Вы хотите применить изменения?");
        println!("[0] Да");
        println!("[1] Нет");
        println!("[2] Выход");

        let Some(confirm) = read_choice() else {
            continue;
        };

        if confirm != 0 {
            println!("Изменения отменены.");
            pause();
            continue;
        }

        config.local_profiles[choice].outbound = new_outbound.clone();

        println!();
        println!(
            "Применяем обновление правил для {} на {}...",
            old_tag, new_outbound
        );

        if let Err(e) = save_config(path, &config) {
            println!("Ошибка сохранения конфига: {e}");
            pause();
            continue;
        }

        let _ = stop_smartroute();
        match start_smartroute(path) {
            Ok(_) => println!("Успешно"),
            Err(e) => println!("Конфиг сохранён, но SmartRoute не запустился: {e}"),
        }

        pause();
    }
}
