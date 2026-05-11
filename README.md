# SmartRoute

<div align="center">

[![CI](https://github.com/PA3MA3AH/smartroute/workflows/CI/badge.svg)](https://github.com/PA3MA3AH/smartroute/actions)
[![Release](https://img.shields.io/github/v/release/PA3MA3AH/smartroute)](https://github.com/PA3MA3AH/smartroute/releases)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org/)

**Умная маршрутизация трафика через разные proxy с гибкими правилами**

[О проекте](#-о-проекте) • [Возможности](#-возможности) • [Установка](#-установка) • [Быстрый старт](#-быстрый-старт) • [Команды](#-команды) • [Разработка](#-разработка)

</div>

---

## 📖 О проекте

**SmartRoute** — CLI/TUI-приложение на Rust для умной маршрутизации трафика через разные proxy-ноды.

Проект работает поверх **sing-box** и позволяет удобно управлять:

- глобальным proxy для всего трафика;
- отдельными правилами для сайтов;
- отдельными локальными SOCKS-портами для приложений;
- цепочками proxy из нескольких нод;
- kill-switch для блокировки direct-трафика;
- диагностикой утечек DNS/SNI;
- импортом и обновлением proxy-подписок.

SmartRoute — это **не VPN**, а локальный smart proxy router.

---

## 🧠 Главная модель

```text
[Global proxy] → используется для всего трафика по умолчанию

[S] Site rule   → отдельный proxy/chain для сайта
[A] App profile → отдельный локальный SOCKS-порт для приложения
[C] Chain proxy → цепочка из нескольких proxy-нод
```

### Пример

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

- 🎯 **Site rules** — отдельный proxy или chain для конкретных доменов.
- 📱 **App profiles** — отдельные локальные SOCKS-порты для приложений.
- ⛓️ **Chain proxy** — цепочки из нескольких proxy-нод.
- 🌐 **Global proxy** — дефолтный outbound для всего трафика.
- 🖥️ **TUI-интерфейс** — управление через терминал.
- 🌍 **RU/EN интерфейс** — переключение языка в TUI.

### Безопасность

- 🔒 **Kill-switch**
  - Linux: через `nftables`.
  - Windows: через Windows Firewall.
- 🔍 **DNS leak-test** — проверка DNS-утечек.
- 🕵️ **SNI leak-test** — проверка видимости target domain.
- ✅ **Whitelist-compatible checks** — проверка маскировки трафика.
- 🛡️ **Reality/uTLS masks** — управление fingerprint и SNI.

### Управление

- 📥 **Импорт подписок** — импорт proxy-нод из subscription URL.
- 🔄 **Merge-nodes** — обновление нод без потери rules/chains/profiles.
- 💾 **Backup/restore** — автоматические бэкапы конфигурации.
- 🏥 **Health-check** — проверка работоспособности SmartRoute.
- 🔧 **Auto-repair** — автоматическое восстановление.
- 📋 **Doctor** — валидация конфигурации.
- 🚀 **Autostart**
  - Linux: systemd.
  - Windows: Task Scheduler.

### Технические

- 🦀 **Rust** — быстрый нативный бинарник.
- 📊 **Structured logging** — логи с уровнями `trace/debug/info/warn/error`.
- ⚛️ **Atomic writes** — защита конфигов от повреждения.
- 🧪 **Unit-тесты** — тесты критичной логики.
- 🪟 **Windows support** — сборка, тесты и `.exe` installer.
- 🐧 **Linux support** — systemd, nftables, стандартные CLI tools.

---

## 🚀 Установка

## Windows 10/11 x64

Скачайте последнюю версию установщика из раздела **Releases**:

- `SmartRoute-Setup-x64.exe`

Страница релизов:

```text
https://github.com/PA3MA3AH/smartroute/releases
```

После установки SmartRoute будет доступен из PowerShell/CMD:

```powershell
smartroute.exe --help
```

### Windows runtime-папки

```text
C:\ProgramData\SmartRoute\run
C:\ProgramData\SmartRoute\config
```

### Требования для Windows

- Windows 10/11 x64.
- `sing-box.exe` должен быть установлен и доступен в `PATH`.

Если `sing-box.exe` лежит в отдельной папке, укажите путь вручную:

```powershell
setx SMARTROUTE_SINGBOX "C:\path\to\sing-box.exe" /M
```

После этого перезапустите PowerShell/Terminal.

### Пример запуска на Windows

```powershell
smartroute.exe doctor C:\ProgramData\SmartRoute\config\imported.toml
smartroute.exe start C:\ProgramData\SmartRoute\config\imported.toml
smartroute.exe status
smartroute.exe stop
```

### Windows kill-switch и autostart

Эти команды запускайте из PowerShell или Windows Terminal **от имени администратора**:

```powershell
smartroute.exe kill-switch enable C:\ProgramData\SmartRoute\config\imported.toml
smartroute.exe kill-switch status
smartroute.exe kill-switch disable
```

```powershell
smartroute.exe autostart enable C:\ProgramData\SmartRoute\config\imported.toml
smartroute.exe autostart status
smartroute.exe autostart disable
```

---

## Linux

### Arch Linux

```bash
sudo pacman -S --needed rust cargo git curl sing-box nftables iproute2
```

### Debian/Ubuntu

```bash
sudo apt install rust cargo git curl nftables iproute2
```

`sing-box` для Debian/Ubuntu может потребоваться установить отдельно:

```text
https://sing-box.sagernet.org/
```

### Сборка из исходников

```bash
git clone https://github.com/PA3MA3AH/smartroute.git
cd smartroute

cargo build --release

sudo cp target/release/smartroute /usr/local/bin/
```

Проверка:

```bash
smartroute --help
```

### Быстрая установка через install.sh

> Важно: `install.sh` ожидает Linux binary artifact в GitHub Releases. Если в релизе есть только Windows installer, используйте сборку из исходников.

```bash
curl -fsSL https://raw.githubusercontent.com/PA3MA3AH/smartroute/master/install.sh | sudo bash
```

или:

```bash
wget -qO- https://raw.githubusercontent.com/PA3MA3AH/smartroute/master/install.sh | sudo bash
```

---

## 🎯 Быстрый старт

### 1. Создайте конфиг

Linux:

```bash
sudo mkdir -p /etc/smartroute
sudo nano /etc/smartroute/config.toml
```

Windows:

```powershell
notepad C:\ProgramData\SmartRoute\config\config.toml
```

### 2. Минимальный конфиг

```toml
[general]
mode = "socks"
listen = "127.0.0.1"
listen_port = 1081
final_outbound = "my-proxy"

[[nodes]]
tag = "my-proxy"
type = "vless"
server = "example.com"
port = 443
uuid = "your-uuid"
security = "reality"
server_name = "example.com"
reality_public_key = "your-public-key"
reality_short_id = ""
utls_fingerprint = "chrome"
```

### 3. Проверка конфига

Linux:

```bash
smartroute doctor /etc/smartroute/config.toml
```

Windows:

```powershell
smartroute.exe doctor C:\ProgramData\SmartRoute\config\config.toml
```

### 4. Запуск

Linux:

```bash
sudo smartroute start /etc/smartroute/config.toml
```

Windows:

```powershell
smartroute.exe start C:\ProgramData\SmartRoute\config\config.toml
```

### 5. Проверка статуса

```bash
smartroute status
```

На Windows:

```powershell
smartroute.exe status
```

---

## 🖥️ TUI

SmartRoute имеет терминальный интерфейс управления.

Linux:

```bash
sudo smartroute ui
```

Windows:

```powershell
smartroute.exe ui
```

В TUI можно:

- запускать/останавливать SmartRoute;
- включать daemon;
- менять global proxy;
- добавлять site rules;
- создавать app profiles;
- создавать chain proxy;
- смотреть rules/chains/app ports;
- импортировать subscription URL;
- запускать проверки.

---

## 📚 Команды

### Управление runtime

```bash
smartroute start <config>
smartroute stop
smartroute status
smartroute daemon <config>
```

### TUI

```bash
smartroute ui
```

### Проверка и восстановление

```bash
smartroute doctor <config>
smartroute health <config>
smartroute repair <config>
```

### Правила маршрутизации

```bash
smartroute rule add <config> domain_suffix youtube.com my-proxy
smartroute rule list <config>
```

### Импорт подписок

```bash
smartroute import-url --output nodes.toml "https://example.com/sub"
smartroute merge-nodes base.toml nodes.toml -o base.toml
```

### Backup/restore

```bash
smartroute backup <config>
smartroute restore <config> --latest
```

### Kill-switch

Linux:

```bash
sudo smartroute kill-switch enable <config>
sudo smartroute kill-switch status
sudo smartroute kill-switch disable
```

Windows, PowerShell от администратора:

```powershell
smartroute.exe kill-switch enable C:\ProgramData\SmartRoute\config\config.toml
smartroute.exe kill-switch status
smartroute.exe kill-switch disable
```

### Autostart

Linux:

```bash
sudo smartroute autostart enable <config>
sudo smartroute autostart status
sudo smartroute autostart disable
```

Windows, PowerShell от администратора:

```powershell
smartroute.exe autostart enable C:\ProgramData\SmartRoute\config\config.toml
smartroute.exe autostart status
smartroute.exe autostart disable
```

### Leak tests

Linux:

```bash
sudo smartroute leak-test <config> --domain youtube.com -i eth0
sudo smartroute dns-test <config> --domain youtube.com -i eth0
sudo smartroute whitelist test <config> --domain youtube.com -i eth0
```

> Некоторые leak-test команды требуют `tcpdump`, `tshark` и root-доступ.

---

## ⚙️ Конфигурация

### Общий блок

```toml
[general]
mode = "socks"
listen = "127.0.0.1"
listen_port = 1081
final_outbound = "my-proxy"
```

### Node

```toml
[[nodes]]
tag = "my-proxy"
type = "vless"
server = "example.com"
port = 443
uuid = "your-uuid"
security = "reality"
server_name = "example.com"
reality_public_key = "your-public-key"
reality_short_id = ""
utls_fingerprint = "chrome"
```

### Site rule

```toml
[[rules]]
type = "domain_suffix"
value = "youtube.com"
outbound = "my-proxy"
```

### Chain proxy

```toml
[[chains]]
tag = "my-chain"
outbounds = ["proxy-1", "proxy-2"]
```

### App profile

```toml
[[local_profiles]]
tag = "browser"
listen = "127.0.0.1"
listen_port = 1082
outbound = "my-chain"
```

После запуска приложение может использовать отдельный локальный SOCKS-порт:

```text
127.0.0.1:1082
```

---

## 🌍 Переменные окружения

### Логирование

Linux:

```bash
RUST_LOG=debug sudo smartroute start config.toml
```

Windows PowerShell:

```powershell
$env:RUST_LOG="debug"
smartroute.exe start C:\ProgramData\SmartRoute\config\config.toml
```

Доступные уровни:

```text
trace
debug
info
warn
error
```

### Путь к sing-box

Linux:

```bash
export SMARTROUTE_SINGBOX="/usr/bin/sing-box"
```

Windows:

```powershell
setx SMARTROUTE_SINGBOX "C:\path\to\sing-box.exe" /M
```

---

## 🛠️ Разработка

### Сборка

```bash
cargo build
cargo build --release
```

### Тесты

```bash
cargo test
cargo test --lib
```

### Форматирование

```bash
cargo fmt
```

### Проверка

```bash
cargo check --all-targets
```

### Сборка Windows installer

Windows installer собирается GitHub Actions workflow:

```text
Actions → Windows Installer → Run workflow
```

Результат появляется в artifacts:

```text
SmartRoute-Setup-x64.exe
```

Для релиза создайте tag:

```bash
git tag v0.1.0
git push origin v0.1.0
```

Workflow автоматически:

1. Соберёт `smartroute.exe`.
2. Соберёт `SmartRoute-Setup-x64.exe`.
3. Создаст GitHub Release.
4. Прикрепит installer к релизу.

---

## 🧪 CI

CI проверяет проект на Linux и Windows:

```text
cargo fmt
cargo check --all-targets
cargo test --all
```

Windows installer workflow собирает отдельный `.exe` установщик.

---

## ⚠️ Важные замечания

- SmartRoute не является VPN.
- SmartRoute работает как локальный proxy router.
- Для работы требуется `sing-box`.
- На Windows `kill-switch` использует Windows Firewall.
- На Linux `kill-switch` использует `nftables`.
- Для `kill-switch` и `autostart` обычно нужны права администратора/root.
- Windows installer не включает `sing-box.exe`; установите его отдельно.

---

## 🤝 Поддержка

- **Telegram:** [@PA3MA3AH](https://t.me/PA3MA3AH)
- **Issues:** [GitHub Issues](https://github.com/PA3MA3AH/smartroute/issues)
- **Discussions:** [GitHub Discussions](https://github.com/PA3MA3AH/smartroute/discussions)

---

## 📝 Лицензия

Этот проект распространяется под лицензией MIT.

Подробности в файле [LICENSE](LICENSE).

---

## 🙏 Благодарности

- [sing-box](https://sing-box.sagernet.org/) — универсальная proxy-платформа.
- [ratatui](https://github.com/ratatui-org/ratatui) — TUI-фреймворк.
- [crossterm](https://github.com/crossterm-rs/crossterm) — терминальный backend.
- Всем контрибьюторам проекта.

---

<div align="center">

**Сделано с ❤️ на Rust**

[⬆ Наверх](#smartroute)

</div>
