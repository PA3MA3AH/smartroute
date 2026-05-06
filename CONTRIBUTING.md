# Contributing to SmartRoute

Спасибо за интерес к SmartRoute! Мы рады любому вкладу в проект.

## 🚀 Как начать

### 1. Fork и клонирование

```bash
# Fork репозиторий через GitHub UI
git clone https://github.com/YOUR_USERNAME/smartroute.git
cd smartroute
git remote add upstream https://github.com/PA3MA3AH/smartroute.git
```

### 2. Создание ветки

```bash
git checkout -b feature/your-feature-name
# или
git checkout -b fix/your-bug-fix
```

### 3. Разработка

```bash
# Установка зависимостей
cargo build

# Запуск тестов
cargo test

# Форматирование
cargo fmt

# Линтинг
cargo clippy
```

### 4. Коммит

Используйте понятные commit messages:

```bash
git commit -m "Add feature: description"
git commit -m "Fix: bug description"
git commit -m "Docs: update README"
```

**Формат коммитов:**
- `Add:` — новая функциональность
- `Fix:` — исправление бага
- `Refactor:` — рефакторинг без изменения функциональности
- `Docs:` — изменения в документации
- `Test:` — добавление/изменение тестов
- `Chore:` — обновление зависимостей, CI/CD и т.д.

### 5. Push и Pull Request

```bash
git push origin feature/your-feature-name
```

Затем создайте Pull Request через GitHub UI.

---

## 📋 Checklist перед PR

- [ ] Код отформатирован (`cargo fmt`)
- [ ] Нет предупреждений clippy (`cargo clippy`)
- [ ] Все тесты проходят (`cargo test`)
- [ ] Добавлены тесты для новой функциональности
- [ ] Обновлена документация (если нужно)
- [ ] Commit messages понятные и информативные

---

## 🧪 Тестирование

### Запуск всех тестов

```bash
cargo test
```

### Запуск конкретного теста

```bash
cargo test test_name
```

### Тесты с выводом

```bash
cargo test -- --nocapture
```

### Покрытие кода (опционально)

```bash
cargo install cargo-tarpaulin
cargo tarpaulin --out Html
```

---

## 📝 Стиль кода

### Rust

Следуем стандартному Rust style guide:
- Используем `rustfmt` для форматирования
- Следуем рекомендациям `clippy`
- Избегаем `unwrap()` в production коде (используем `?` или `expect()`)
- Добавляем doc-комментарии для публичных API

### Именование

```rust
// Функции и переменные: snake_case
fn load_config() -> Result<Config> { ... }
let user_name = "test";

// Типы и трейты: PascalCase
struct SmartRouteConfig { ... }
trait ConfigLoader { ... }

// Константы: SCREAMING_SNAKE_CASE
const MAX_RETRIES: u32 = 3;
```

### Комментарии

```rust
// Краткие комментарии для неочевидной логики
let timeout = Duration::from_secs(10); // Wait for port availability

/// Doc-комментарии для публичных функций
/// 
/// # Arguments
/// * `path` - Path to config file
/// 
/// # Returns
/// Loaded configuration or error
pub fn load_config(path: &Path) -> Result<Config> { ... }
```

---

## 🐛 Сообщение о багах

При создании issue укажите:

1. **Версия SmartRoute:** `smartroute --version`
2. **ОС и версия:** `uname -a`
3. **Шаги для воспроизведения**
4. **Ожидаемое поведение**
5. **Фактическое поведение**
6. **Логи:** `RUST_LOG=debug smartroute ...`

---

## 💡 Предложение функций

При предложении новой функции опишите:

1. **Проблему:** Какую проблему решает?
2. **Решение:** Как это должно работать?
3. **Альтернативы:** Рассматривали ли другие варианты?
4. **Примеры использования:** Как это будет использоваться?

---

## 🔍 Code Review

Все PR проходят code review. Мы проверяем:

- Соответствие стилю кода
- Наличие тестов
- Качество документации
- Производительность
- Безопасность

Будьте готовы к обсуждению и внесению изменений.

---

## 📞 Связь

- **Telegram:** [@PA3MA3AH](https://t.me/PA3MA3AH)
- **GitHub Discussions:** [Обсуждения](https://github.com/PA3MA3AH/smartroute/discussions)
- **Issues:** [Баг-трекер](https://github.com/PA3MA3AH/smartroute/issues)

---

## 📜 Лицензия

Внося вклад в проект, вы соглашаетесь с тем, что ваш код будет распространяться под лицензией MIT.

---

Спасибо за вклад в SmartRoute! 🚀
