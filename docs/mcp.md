# xmath и MCP: управление расчётами из любой нейросети

`xmath-mcp` — это MCP-сервер (Model Context Protocol) для xmath. Он работает по stdio
(JSON-RPC 2.0, по одному JSON-сообщению в строке) и даёт любой LLM-программе (Claude Code,
Claude Desktop, Cursor и т. д.) инструменты для расчётов с единицами измерения и для
создания/редактирования рабочих листов `.xmath` в стиле Mathcad.

## Сборка и установка

Нужен Rust 1.80 или новее.

```bash
git clone https://github.com/t1murjan/xmath
cd xmath
cargo build --release -p xmath-mcp
# бинарник: target/release/xmath-mcp  (на Windows: xmath-mcp.exe)

# установка в ~/.cargo/bin (должен быть в PATH):
cargo install --path crates/xmath-mcp
```

Проверка вручную (сервер читает запросы из stdin и отвечает в stdout):

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"evaluate","arguments":{"lines":["L := 6 m","q := 12 kN/m","M := q*L^2/8","M = kN*m"]}}}' \
  | xmath-mcp
```

В ответе на последнюю строку будет `M = kN*m  ->  54 kN*m`. Диагностика сервера пишется только
в stderr, поэтому stdout остаётся чистым протоколом.

## Подключение

### Claude Code

```bash
claude mcp add xmath -- xmath-mcp
# если бинарник не в PATH — полный путь:
claude mcp add xmath -- /полный/путь/к/target/release/xmath-mcp
# для всех проектов пользователя:
claude mcp add --scope user xmath -- xmath-mcp
```

Проверка: `claude mcp list`, внутри сессии — команда `/mcp`.

Относительные пути к файлам `.xmath` считаются от рабочей директории сервера
(для Claude Code это директория проекта).

### Claude Desktop

Откройте `claude_desktop_config.json`
(macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`,
Windows: `%APPDATA%\Claude\claude_desktop_config.json`) и добавьте:

```json
{
  "mcpServers": {
    "xmath": {
      "command": "/полный/путь/к/xmath-mcp",
      "args": []
    }
  }
}
```

На Windows слэши в пути удваиваются: `"C:\\tools\\xmath-mcp.exe"`. Перезапустите Claude Desktop.

### Cursor и другие MCP-клиенты

Cursor: `~/.cursor/mcp.json` (или `.cursor/mcp.json` в проекте). Любой клиент, который
запускает MCP-серверы как процессы, использует тот же формат:

```json
{
  "mcpServers": {
    "xmath": {
      "command": "xmath-mcp",
      "args": [],
      "env": {}
    }
  }
}
```

Требования к клиенту минимальные: запустить команду и общаться через stdin/stdout.
Поддерживаемые версии протокола: `2025-06-18`, `2025-03-26`, `2024-11-05`.

## Инструменты

| Инструмент | Назначение |
|------------|-----------|
| `evaluate` | вычислить строки (`lines` или `text`), по региону на строку; возвращает результат или ошибку для каждой строки, ничего не сохраняет |
| `open_worksheet` | загрузить `.xmath` или `.xmcd` (Mathcad 15), вычислить, вернуть все регионы (id, тип, позиция, текст, результат, ошибка, сводка графика) |
| `create_worksheet` | создать `.xmath` или `.xmcd` (`path`, `lines?`, `overwrite?`) |
| `add_region` | добавить регион `math` / `text` / `plot` (в конец или после `after_id`, либо в позицию `x`, `y`) |
| `update_region` | изменить регион по `id` (`source`, `text`, `x_expr`, `y_exprs`, `x`, `y`) |
| `delete_region` | удалить регион по `id` |
| `export_pdf` | `path` (.xmath/.xmcd) → `out` (.pdf): лист в PDF, постранично |
| `export_xmcd` | `path` → `out` (.xmcd): лист в формате Mathcad 15 |
| `list_functions` | встроенные функции с описаниями |
| `list_units` | единицы измерения по размерностям |
| `syntax_help` | полный справочник по синтаксису |

Запись разрешена только в файлы с расширением `.xmath` и `.xmcd`. Работа с `.xmcd` напрямую: `open_worksheet`,
`add_region`, `update_region` и `delete_region` читают файл Mathcad импортом и сохраняют экспортом (графики X-Y —
настоящие графики Mathcad с картинкой, полярные и 3D — рисунком с данными xmath в комментарии, для вычисляемых
областей записываются результаты), так что ИИ-агент может править файлы Mathcad. Не трогайте `.xmcd`, которые вы
не хотите переписать: сохранение перезаписывает весь файл представлением xmath. Ошибки инструментов возвращаются
как результат с `isError: true` (модель видит текст и может исправить вызов), ошибки протокола —
как JSON-RPC ошибки.

Пример работы модели:

```text
evaluate  lines: ["L := 6 m", "q := 12 kN/m", "M := q*L^2/8", "M = kN*m"]
  -> M = kN*m  ->  54 kN*m

create_worksheet  path: "beam.xmath"  lines: ["# Балка", "L := 6 m", "q := 12 kN/m", "M := q*L^2/8", "M = kN*m"]
add_region        path: "beam.xmath"  kind: "plot"  x_expr: "x"  y_exprs: ["x^2"]
```

## Agent Skill

Skill `skills/xmath` учит модель синтаксису xmath, порядку работы с инструментами и типичным
ошибкам. Установка для Claude Code:

```bash
# для всех проектов
mkdir -p ~/.claude/skills
cp -r skills/xmath ~/.claude/skills/

# или только для одного проекта
mkdir -p .claude/skills
cp -r skills/xmath .claude/skills/
```

Windows (PowerShell):

```powershell
Copy-Item -Recurse skills\xmath $env:USERPROFILE\.claude\skills\
```

Структура: `skills/xmath/SKILL.md` (когда использовать, рабочий процесс, примеры, подводные камни)
и `skills/xmath/reference.md` (полный справочник; тот же текст отдаёт инструмент `syntax_help`).

## Без MCP: командная строка

```bash
cargo install --path crates/xmath-cli
printf 'L := 6 m\nq := 12 kN/m\nM := q*L^2/8\nM = kN*m\n' > calc.txt
xmath-cli calc.txt        # результаты по строкам; код возврата 1, если есть ошибки
xmath-cli sheet.xmath
```

## Частые проблемы

- `command not found` — укажите полный путь к `xmath-mcp` в конфигурации клиента.
- Сервер «молчит» — это нормально: он отвечает только на запросы клиента; логи смотрите в stderr клиента.
- `refusing to write ... only .xmath` — сервер пишет только в `.xmath`.
- Ошибка единиц (`Единицы измерения не совпадают`) — формула размерно неверна; не убирайте единицы, исправьте формулу.

## Работа в открытом окне программы

Каждое окно xmath слушает локальный сокет `$XDG_RUNTIME_DIR/xmath/<pid>.sock` (протокол — строки JSON,
см. `crates/xmath-core/src/live.rs`). Инструмент `list_open_windows` перечисляет окна (последнее активное —
первым), а `path: "window"` / `"window:<pid>"` вместо пути к файлу направляет `open_worksheet`, `add_region`,
`update_region`, `delete_region`, `create_worksheet` и экспорт в открытый документ, в том числе несохранённый.
Каждое изменение — один шаг отмены (Ctrl+Z) в окне; файл сохраняет сам пользователь.

## OpenCode (DeepSeek и другие модели)

`install.sh` добавляет в `~/.config/opencode/opencode.json`:

```json
{ "mcp": { "xmath": { "type": "local", "command": ["/home/<вы>/.local/bin/xmath-mcp"], "enabled": true } } }
```

Навык OpenCode берёт из `~/.claude/skills/xmath`. Если в файле настроек есть комментарии (JSONC),
установщик его не трогает и показывает строку для ручного добавления.

