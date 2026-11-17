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

#### Программы (палитра «Программирование»)

Программа Mathcad записывается в одну строку в фигурных скобках; операторы разделяются `;`
(в Mathcad это столбец строк с вертикальной чертой слева — xmath рисует её так же).
Программа — обычное выражение: ею можно определять переменные и функции.

| Запись | Значение |
|---|---|
| `x ← expr` (или `x <- expr`), `v[i] ← expr` | локальное присваивание (локальные имена затеняют переменные листа) |
| `for i ∈ 1..n { ... }` (или `for i in 1..n`) | цикл по диапазону или вектору |
| `while условие { ... }` | цикл с условием |
| `if условие { ... } otherwise { ... }` (также `else`, `else if`) | ветвление |
| `return -1 if x < 0` | оператор с условием в стиле Mathcad (`stmt if условие`) |
| `break`, `continue`, `return expr` | управление циклом / досрочный выход |
| `expr1 on error expr2` | значение `expr2`, если вычисление `expr1` завершилось ошибкой |

Значение программы — значение последнего выполненного оператора (или `return`).
Предел числа итераций цикла — 10 000 000 за один запуск программы.

```
f(n) := { s ← 0; for i ∈ 1..n { s ← s + i^2 }; s }
g(x) := { return -1 if x < 0; sqrt(x) }
h(x) := { if x < 0 { "neg" } else if x == 0 { "zero" } otherwise { "pos" } }
k(x) := { y ← 1/x on error 0; y }
w(a) := { i ← 0; while i < a { i ← i + 1; if i == 5 { break } }; i }
```

Пример: `examples/programs.xmath`. Программы из `.xmcd` (Mathcad 15) импортируются в этот же синтаксис.
Сравнение с отрицательным числом пишите с пробелом: `a < -1` (запись `a<-1` читается как присваивание `←`).

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

## ИИ и MCP

xmath можно подключить к ИИ-ассистентам: описание MCP-сервера — [docs/mcp.md](docs/mcp.md),
навык для моделей — [skills/xmath/SKILL.md](skills/xmath/SKILL.md).
Подключение к Claude Code: `claude mcp add xmath -- ~/.local/bin/xmath-mcp`;
навык ставится командой `install.sh --with-skill`.

## Планы

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
