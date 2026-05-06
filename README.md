# SmartRoute

<div align="center">

[![CI](https://github.com/PA3MA3AH/smartroute/workflows/CI/badge.svg)](https://github.com/PA3MA3AH/smartroute/actions)
[![Release](https://img.shields.io/github/v/release/PA3MA3AH/smartroute)](https://github.com/PA3MA3AH/smartroute/releases)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)

**Умная маршрутизация трафика через разные proxy с гибкими правилами**

[Возможности](#-возможности) • [Установка](#-установка) • [Быстрый старт](#-быстрый-старт) • [Документация](#-документация) • [Поддержка](#-поддержка)

</div>

---

## 📖 О проекте

**SmartRoute** — CLI/TUI-приложение на Rust для умной маршрутизации трафика через разные proxy-ноды с поддержкой site rules, app profiles и chain proxy.

### Главная модель

```text
[Global proxy] → используется для всего трафика по умолчанию

[S] Site rule   → отдельный proxy/chain для сайта
[A] App profile → отдельный локальный SOCKS-порт для приложения
[C] Chain proxy → цепочка из нескольких proxy-нод
```

### Пример конфигурации

```text
[Global] → ru-chain

[S] youtube.com → 5-gbit-tcp-3
[S] github.com  → chain "git"

[A] Steam       → chain "gaming"
[A] Browser     → local SOCKS port 127.0.0.1:1082

[C] ru-chain    → tcp-1 → youtube
[C] git         → 5-gbit-3 → 5-gbit
```

---

## ✨ Возможности

### Основные
- 🎯 **Site rules** — отдельный proxy для конкретных доменов
- 📱 **App profiles** — изолированные SOCKS-порты для приложений
- ⛓️ **Chain proxy** — цепочки из нескольких proxy-нод
- 🌐 **Global proxy** — дефолтный proxy для всего трафика
- 🖥️ **TUI-интерфейс** — удобное управление через терминал

### Безопасность
- 🔒 **Kill-switch** — блокировка direct трафика через nftables
- 🔍 **DNS leak-test** — проверка утечек DNS
- 🕵️ **SNI leak-test** — проверка видимости target domain
- ✅ **Whitelist-compatible** — проверка маскировки трафика
- 🛡️ **Reality/uTLS masks** — управление fingerprint и SNI

### Управление
- 📥 **Импорт подписок** — автоматическое добавление нод
- 🔄 **Merge-nodes** — обновление нод без потери rules/chains
- 💾 **Backup/restore** — автоматические бэкапы конфигурации
- 🏥 **Health-check** — проверка работоспособности
- 🔧 **Auto-repair** — автоматическое восстановление
- 📋 **Doctor** — валидация конфигурации

### Технические
- 📊 **Structured logging** — логи с уровнями и фильтрацией
- ⚛️ **Atomic writes** — защита от повреждения конфигов
- 🧪 **46 unit-тестов** — покрытие критичной логики
- 🚀 **Systemd integration** — автозапуск и управление через systemctl

---

## 🚀 Установка

### Быстрая установка (рекомендуется)

```bash
curl -fsSL https://raw.githubusercontent.com/PA3MA3AH/smartroute/master/install.sh | sudo bash
```

или

```bash
wget -qO- https://raw.githubusercontent.com/PA3MA3AH/smartroute/master/install.sh | sudo bash
```

### Ручная установка

#### 1. Установка зависимостей

**Arch Linux:**
```bash
sudo pacman -S --needed rust cargo git curl sing-box nftables iproute2
```

**Debian/Ubuntu:**
```bash
sudo apt install rust cargo git curl nftables iproute2
# sing-box нужно установить отдельно: https://sing-box.sagernet.org/
```

#### 2. Сборка из исходников

```bash
git clone https://github.com/PA3MA3AH/smartroute.git
cd smartroute
cargo build --release
sudo cp target/release/smartroute /usr/local/bin/
```

#### 3. Установка systemd service (опционально)

```bash
sudo cp systemd/smartroute.service /etc/systemd/system/
sudo cp systemd/smartroute@.service /etc/systemd/system/
sudo systemctl daemon-reload
```

---

## 🎯 Быстрый старт

### 1. Создание конфигурации

```bash
sudo mkdir -p /etc/smartroute
sudo nano /etc/smartroute/config.toml
```

Минимальный конфиг:
```toml
[general]
mode = "socks"
listen = "127.0.0.1"
listen_port = 1081
final_outbound = "direct"

[[nodes]]
tag = "my-proxy"
type = "vless"
server = "example.com"
port = 443
uuid = "your-uuid"
security = "reality"
server_name = "example.com"
```

### 2. Запуск

**Разовый запуск:**
```bash
sudo smartroute start /etc/smartroute/config.toml
```

**Через systemd:**
```bash
sudo systemctl start smartroute
sudo systemctl enable smartroute  # автозапуск
```

**TUI-интерфейс:**
```bash
sudo smartroute ui
```

### 3. Проверка статуса

```bash
sudo smartroute status
# или
sudo systemctl status smartroute
```

---

## 📚 Документация

### Основные команды

```bash
# Управление
smartroute start <config>     # Запустить SmartRoute
smartroute stop               # Остановить SmartRoute
smartroute status             # Проверить статус
smartroute daemon <config>    # Запустить daemon с self-heal

# TUI
smartroute ui                 # Открыть TUI-интерфейс

# Конфигурация
smartroute doctor <config>    # Проверить конфиг
smartroute health <config>    # Health-check
smartroute repair <config>    # Auto-repair

# Правила
smartroute rule add <config> domain_suffix youtube.com my-proxy
smartroute rule list <config>

# Импорт подписок
smartroute import-url --output nodes.toml 'https://example.com/sub'
smartroute merge-nodes base.toml nodes.toml -o base.toml

# Backup/Restore
smartroute backup <config>
smartroute restore <config> --latest

# Тестирование
smartroute leak-test <config> --domain youtube.com -i eth0
smartroute dns-test <config> --domain youtube.com -i eth0
smartroute whitelist test <config> --domain youtube.com -i eth0

# Kill-switch
smartroute kill-switch enable <config>
smartroute kill-switch disable
smartroute kill-switch status
```

### Переменные окружения

```bash
# Уровень логирования (trace, debug, info, warn, error)
RUST_LOG=debug sudo smartroute start config.toml

# Только ошибки
RUST_LOG=error sudo smartroute start config.toml
```

### Примеры конфигураций

Смотри [examples/](examples/) для готовых примеров:
- `basic.toml` — минимальная конфигурация
- `advanced.toml` — с rules, chains, profiles
- `gaming.toml` — оптимизация для игр

---

## 🔧 Разработка

### Сборка

```bash
cargo build              # debug
cargo build --release    # release
```

### Тесты

```bash
cargo test               # все тесты
cargo test --lib         # только unit-тесты
```

### Форматирование и линтинг

```bash
cargo fmt                # форматирование
cargo clippy             # линтер
```

### Коммит

```bash
git add .
git commit -m "Your message"
git push
```

---

## 🤝 Поддержка

- **Telegram:** [@PA3MA3AH](https://t.me/PA3MA3AH)
- **Issues:** [GitHub Issues](https://github.com/PA3MA3AH/smartroute/issues)
- **Discussions:** [GitHub Discussions](https://github.com/PA3MA3AH/smartroute/discussions)

---

## 📝 Лицензия

Этот проект распространяется под лицензией MIT. Подробности в файле [LICENSE](LICENSE).

---

## 🙏 Благодарности

- [sing-box](https://sing-box.sagernet.org/) — мощный универсальный прокси-платформа
- [ratatui](https://github.com/ratatui-org/ratatui) — TUI фреймворк
- Всем контрибьюторам проекта

---

<div align="center">

**Сделано с ❤️ на Rust**

[⬆ Наверх](#smartroute)

</div>
