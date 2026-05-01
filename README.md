# SmartRoute

**SmartRoute** — CLI/TUI-приложение на Rust для умной маршрутизации трафика через разные proxy-ноды, site rules, app profiles и chain proxy.

Главная модель проекта:

```text
[Global proxy] -> используется для всего трафика по умолчанию

[S] Site rule   -> отдельный proxy/chain для сайта
[A] App profile -> отдельный локальный SOCKS-порт для приложения
[C] Chain proxy -> цепочка из нескольких proxy-нод
```

Пример логики:

```text
[Global] -> ru-chain

[S] youtube.com -> 5-gbit-tcp-3
[S] github.com  -> chain "git"

[A] Steam       -> chain "gaming"
[A] Browser     -> local SOCKS port 127.0.0.1:1082

[C] ru-chain    -> tcp-1 -> youtube
[C] git         -> 5-gbit-3 -> 5-gbit
```

---

## Возможности

- TUI-интерфейс для управления SmartRoute.
- Запуск локального SOCKS5-роутера.
- Site rules для отдельных доменов.
- App profiles через отдельные локальные SOCKS-порты.
- Chain proxy.
- Global proxy через `final_outbound`.
- Импорт подписок.
- Обновление nodes без потери своих rules/chains через `merge-nodes`.
- Kill-switch через nftables.
- Проверка конфигурации через doctor.
- Health-check и auto-repair.
- DNS leak-test.
- SNI / traffic leak-test.
- Whitelist-compatible route test.
- Backup/restore конфигурации.
- Reality/uTLS mask controls.

---

## Онлайн-поддержка

Telegram: **@PA3MA3AH**

---

## Зависимости

### Arch Linux

```bash
sudo pacman -S --needed rust cargo git curl sing-box nftables iproute2 tcpdump wireshark-cli
```

Опционально:

```bash
sudo pacman -S --needed micro
```

---

## Сборка

```bash
git clone https://github.com/PA3MA3AH/smartroute.git
cd smartroute

cargo build --release
```

Готовый бинарник:

```bash
./target/release/smartroute
```

---

## Первый запуск

SmartRoute использует TOML-конфиг.

Пример запуска:

```bash
sudo ./target/release/smartroute start imported.toml
```

Остановка:

```bash
sudo ./target/release/smartroute stop
```

Статус:

```bash
./target/release/smartroute status
```

---

## TUI-интерфейс

Запуск TUI:

```bash
sudo ./target/release/smartroute ui
```

В TUI можно:

- запускать и останавливать SmartRoute;
- запускать daemon;
- менять proxy для сайтов;
- менять proxy для приложений;
- создавать chain proxy;
- импортировать proxy по ссылке;
- менять путь к конфигу;
- смотреть rules;
- смотреть Reality/uTLS masks;
- запускать leak-test;
- запускать DNS leak-test;
- запускать doctor;
- запускать health-check;
- запускать repair;
- смотреть whitelist-compatible masks;
- проверять whitelist route;
- создавать backup;
- восстанавливать backup.

### Управление в TUI

```text
↑ / ↓       выбор пункта
k / j       выбор пункта
Enter       выполнить пункт
q / Esc     выход
```

---

## Основная модель конфигурации

### Global proxy

`final_outbound` используется для всего трафика, который не попал под отдельные rules.

```toml
[general]
mode = "socks"
listen = "127.0.0.1"
listen_port = 1081
final_outbound = "ru-chain"
```

### Site rules

Правила для сайтов:

```toml
[[rules]]
type = "domain_suffix"
value = "youtube.com"
outbound = "5-gbit-tcp-3"
```

Пример добавления:

```bash
./target/release/smartroute rule add imported.toml domain_suffix youtube.com 5-gbit-tcp-3 -o imported.toml
```

Список правил:

```bash
./target/release/smartroute rule list imported.toml
```

### App profiles

App profile — отдельный локальный SOCKS-порт для конкретного приложения.

Пример:

```toml
[[local_profiles]]
tag = "steam"
listen = "127.0.0.1"
listen_port = 1082
outbound = "gaming"
```

После этого приложение можно настроить на:

```text
SOCKS5 127.0.0.1:1082
```

### Chain proxy

Chain proxy — цепочка из нескольких outbounds:

```toml
[[chains]]
tag = "ru-chain"
outbounds = ["tcp-1", "youtube"]
```

---

## Импорт подписки

Импорт подписки в отдельный файл:

```bash
ALL_PROXY=socks5h://127.0.0.1:1081 \
HTTPS_PROXY=socks5h://127.0.0.1:1081 \
./target/release/smartroute import-url --output imported-new.toml 'https://example.com/subscription'
```

Важно: не рекомендуется сразу перезаписывать рабочий `imported.toml`, потому что импорт подписки может создать чистый конфиг без ваших rules/chains.

Правильная схема:

```text
imported.toml      -> рабочий конфиг с rules/chains/final_outbound
imported-new.toml  -> свежие nodes из подписки
```

---

## Обновление nodes без потери правил

После импорта подписки можно обновить только nodes:

```bash
./target/release/smartroute merge-nodes imported.toml imported-new.toml -o imported.toml
```

Эта команда:

- заменяет `[[nodes]]`;
- сохраняет `rules`;
- сохраняет `chains`;
- сохраняет `local_profiles`;
- сохраняет `final_outbound`;
- сохраняет пользовательскую маршрутизацию;
- делает backup перед записью.

Проверка после merge:

```bash
./target/release/smartroute doctor imported.toml
```

---

## Backup и restore

Создать backup:

```bash
./target/release/smartroute backup imported.toml
```

Показать backups:

```bash
./target/release/smartroute backups imported.toml
```

Восстановить последний backup:

```bash
./target/release/smartroute restore imported.toml --latest
```

Backup-файлы хранятся в:

```text
~/.local/state/smartroute/backups/
```

---

## Kill-switch

Включить kill-switch:

```bash
sudo ./target/release/smartroute kill-switch enable imported.toml
```

Отключить kill-switch:

```bash
sudo ./target/release/smartroute kill-switch disable
```

Проверить статус:

```bash
sudo ./target/release/smartroute kill-switch status
```

Kill-switch нужен, чтобы direct traffic не уходил мимо SmartRoute.

---

## Проверка здоровья

```bash
sudo ./target/release/smartroute health imported.toml
```

Auto-repair:

```bash
sudo ./target/release/smartroute repair imported.toml
```

---

## Doctor конфига

```bash
./target/release/smartroute doctor imported.toml
```

Строгий режим:

```bash
./target/release/smartroute doctor imported.toml --strict
```

Doctor проверяет:

- TOML-синтаксис;
- валидность SmartRoute-конфига;
- `final_outbound`;
- direct rules;
- дубликаты rules;
- chains;
- nodes;
- Reality/uTLS masks;
- sing-box JSON;
- `sing-box check`.

---

## Leak-test

```bash
sudo ./target/release/smartroute leak-test imported.toml --domain youtube.com -i enp3s0
```

Проверяет:

- включён ли kill-switch;
- не используется ли direct;
- работает ли SOCKS;
- какие IP реально видны в packet capture;
- виден ли настоящий target domain в SNI.

---

## DNS leak-test

```bash
sudo ./target/release/smartroute dns-test imported.toml --domain youtube.com -i enp3s0 --strict
```

Проверяет:

- UDP/TCP 53;
- DNS-over-TLS TCP/853;
- DoH SNI;
- видимость target domain;
- работу SOCKS-маршрута.

---

## Whitelist-compatible route test

```bash
sudo ./target/release/smartroute whitelist test imported.toml --domain youtube.com -i enp3s0
```

Показывает, под какую whitelist-группу попадает видимый SNI.

Пример результата:

```text
Captured SNI:
  sun2-16.userapi.com -> vk

[OK] target domain was not visible in SNI
[OK] all captured SNI names belong to whitelist groups
[OK] whitelist-compatible route detected
```

Показать все whitelist-compatible masks:

```bash
./target/release/smartroute whitelist list imported.toml
```

---

## Reality/uTLS masks

Показать masks:

```bash
./target/release/smartroute mask list imported.toml
```

Изменить fingerprint:

```bash
./target/release/smartroute mask set imported.toml youtube --fingerprint chrome -o imported.toml
```

Изменить `server_name`:

```bash
./target/release/smartroute mask set imported.toml youtube --server-name ozon.ru -o imported.toml
```

Важно: изменение `server_name` у Reality-ноды может сломать подключение, если сервер не поддерживает такой SNI.

---

## Daemon / self-heal

Запуск daemon:

```bash
sudo ./target/release/smartroute daemon imported.toml
```

С self-heal интервалом:

```bash
sudo ./target/release/smartroute daemon imported.toml --heal-interval 20
```

Daemon может:

- следить за SmartRoute runtime;
- перезапускать broken runtime;
- запускать periodic diagnose;
- использовать self-heal.

---

## Проверка после изменений

После любых крупных изменений рекомендуется запускать:

```bash
cargo fmt
cargo build --release

./target/release/smartroute doctor imported.toml
sudo ./target/release/smartroute health imported.toml
sudo ./target/release/smartroute whitelist test imported.toml --domain youtube.com -i enp3s0
sudo ./target/release/smartroute dns-test imported.toml --domain youtube.com -i enp3s0 --strict
```

---

## Приватные данные

Не коммитьте:

```text
imported.toml
imported-new.toml
imported-merged.toml
*.pcap
subscription links
personal proxy links
```

Рекомендуется держать `imported.toml` в `.gitignore`.

---

## Разработка

Сборка:

```bash
cargo build
```

Release-сборка:

```bash
cargo build --release
```

Форматирование:

```bash
cargo fmt
```

Проверка:

```bash
cargo check
```

Коммит:

```bash
git add .
git commit -m "Your message"
```

Push через SmartRoute:

```bash
ALL_PROXY=socks5h://127.0.0.1:1081 git push
```

---

## Лицензия

Пока не указана. Добавьте `LICENSE`, если проект будет распространяться публично.

---

## Support

Telegram: **@PA3MA3AH**
