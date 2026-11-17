# xmath

<img src="assets/xmath.svg" width="96" align="right" alt="xmath">

**xmath** — свободная программа с открытым исходным кодом для инженерных и научных расчётов,
альтернатива **Mathcad 15** (классической версии с меню, панелями инструментов и плавающими палитрами,
а не Mathcad Prime). Работает на Linux, Windows и macOS. Написана на Rust.

> Статус: **alpha**. Ядро расчётов работает, интерфейс и совместимость с Mathcad 15 активно дорабатываются.

## Возможности

- Единицы измерения с автоматической проверкой размерностей (`m`, `kg`, `s`, `N`, `kN`, `Pa`, `MPa`, `J`, `W` …).
- Пользовательские функции, диапазонные переменные, векторы и матрицы.
- Численные интегралы, производные, поиск корней, суммы и произведения.
- Графики X-Y.
- Интерфейс в стиле Mathcad 15: меню, панели инструментов, палитры
  Calculator, Graph, Matrix, Evaluation, Calculus, Boolean, Programming, Greek, Symbolic.
- Импорт файлов Mathcad 15 (`.xmcd`); собственный формат `.xmath` (JSON).
- MCP-сервер и навык для ИИ-ассистентов.

## Установка

Одна команда (нужны `git`, `curl` и компилятор C; Rust будет предложено установить автоматически):

```bash
curl -fsSL https://raw.githubusercontent.com/t1murjan/xmath/main/install.sh | bash
```

Обновление:

```bash
curl -fsSL https://raw.githubusercontent.com/t1murjan/xmath/main/install.sh | bash -s -- update
```

Удаление:

```bash
curl -fsSL https://raw.githubusercontent.com/t1murjan/xmath/main/install.sh | bash -s -- uninstall
```

Или, если репозиторий уже склонирован: `./install.sh [install|update|uninstall]`.
Полезные параметры: `--branch <имя>`, `--repo <url>`, `--prefix <каталог>` (по умолчанию `~/.local`),
`--from-local` (сборка из текущего каталога), `--with-skill`, `--force`, `--purge`, `-y`, `-h`.
Если `~/.local/bin` нет в `PATH`, скрипт подскажет строку для `~/.bashrc`.

Ручная сборка:

```bash
git clone https://github.com/t1murjan/xmath.git
cd xmath
cargo build --release
./target/release/xmath        # графическое приложение
./target/release/xmath-cli файл.xmath
```

## Использование

### Синтаксис

| Запись | Значение |
|---|---|
| `x := 5 m` (клавиша `:`) | присваивание |
| `f(x) := x^2` | определение функции |
| `g ≡ 9.81` (клавиша `~`) | глобальное определение |
| `i := 0..10` (клавиша `;`) | диапазон |
| `v[i] := i^2` | элемент вектора |
| `expr =` | вычислить выражение |
| `F = kN` | вывести результат в kN |
| `[1, 2; 3, 4]` | матрица |
| `\|x\|`, `n!`, `2i` | модуль, факториал, комплексная единица |
| `sum(k, 1, n, k^2)` | сумма |
| `integrate(f(x), x, 0, ∞)` | интеграл |
| `diff(f(x), x)` | производная |
| `root(f(x), x, a, b)` | корень на отрезке |
| `if(c, a, b)` | условие |

Единицы: `m kg s N kN Pa MPa J W …`. Как в Mathcad: `gm` — грамм, `g` — ускорение свободного падения.

### Горячие клавиши

| Клавиши | Действие |
|---|---|
| `:` | `:=` |
| `~` | `≡` |
| `=` | вычислить |
| `;` | `..` (диапазон) |
| `\` | √ |
| `\|` | модуль |
| `?` | производная |
| `&` | интеграл |
| `$` | сумма по диапазону |
| `#` | произведение |
| `Ctrl+=` | булево равно |
| `Ctrl+3` / `Ctrl+9` / `Ctrl+0` | `≠` / `≤` / `≥` |
| `Ctrl+G` | греческая буква |
| `Ctrl+M` | вставить матрицу |
| `Ctrl+E` | вставить функцию |
| `Ctrl+U` | вставить единицу |
| `@` | график X-Y |
| `"` | текстовая область |
| `Tab` | следующий placeholder ■ |
| `F9` | пересчитать |
| `Ctrl+Z` / `Ctrl+Y` | отмена / повтор |
| `Ctrl+N` / `Ctrl+O` / `Ctrl+S` | новый / открыть / сохранить |
| `F1` | справка |

## Единицы измерения

Единицы как в Mathcad 15: `5 kN`, `12 kN/m`, `210 GPa`, `72 km/hr`; результат автоматически
приводится к СИ и упрощается до производной единицы (`N`, `Pa`, `J`, `W`, `V`, `Ω`, ...), а
`expr = unit` показывает его в нужной единице (`M = kN*m`, `σ = MPa`, `p = kgf/cm^2`).
Температура: `K`, `R` - абсолютные; `°C`, `°F` - со смещением (`T := 25 °C` хранит 298.15 K,
`T = °C` показывает 25, `T = °F` - 77); `Δ°C`, `Δ°F` - разности (`T + 10 Δ°C`). Систему единиц
отображения (СИ / MKS / CGS / U.S.) задаёт параметр `unit_system` в `format` файла.
Подробности: [docs/units.md](docs/units.md).

## ИИ и MCP

xmath можно подключить к ИИ-ассистентам: описание MCP-сервера — [docs/mcp.md](docs/mcp.md),
навык для моделей — [skills/xmath/SKILL.md](skills/xmath/SKILL.md).
Подключение к Claude Code: `claude mcp add xmath -- ~/.local/bin/xmath-mcp`;
навык ставится командой `install.sh --with-skill`.

## Планы

- Блоки программирования
- Блоки решения `Given`/`Find`
- Символьный движок
- 3D-графики
- Экспорт в `.xmcd`
- Печать и экспорт в PDF

## Участие в разработке

См. [CONTRIBUTING.md](CONTRIBUTING.md).

## Лицензия

MIT OR Apache-2.0 — на выбор ([LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE)).

---

## English summary

**xmath** is a free, open-source clone of Mathcad 15 (the classic UI, not Prime) written in Rust for
Linux, Windows and macOS. Alpha status. It features unit-aware calculations with dimension checking,
functions, ranges, matrices, numeric integrals/derivatives/roots, X-Y plots, a Mathcad-15-style interface
with palettes, `.xmcd` import, and an MCP server for AI assistants.
Install: `curl -fsSL https://raw.githubusercontent.com/t1murjan/xmath/main/install.sh | bash`
(also supports `update` and `uninstall`). Licensed under MIT OR Apache-2.0.
