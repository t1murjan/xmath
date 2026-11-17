#!/usr/bin/env bash
# xmath — установщик / обновление / удаление.
#   curl -fsSL https://raw.githubusercontent.com/t1murjan/xmath/main/install.sh | bash
set -euo pipefail

REPO="${XMATH_REPO:-https://github.com/t1murjan/xmath.git}"
BRANCH="${XMATH_BRANCH:-main}"
PREFIX="${HOME}/.local"
ACTION="install"
FROM_LOCAL=0
ASSUME_YES=0
FORCE=0
PURGE=0
WITH_SKILL=0

DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
SRC_DIR="$DATA_HOME/xmath/src"
VERSION_FILE="$DATA_HOME/xmath/installed-version"
BINS=(xmath xmath-cli xmath-mcp)

if [ -t 1 ]; then
  C_G=$'\033[32m'; C_Y=$'\033[33m'; C_R=$'\033[31m'; C_0=$'\033[0m'
else
  C_G=""; C_Y=""; C_R=""; C_0=""
fi
info() { printf '%s==>%s %s\n' "$C_G" "$C_0" "$*"; }
warn() { printf '%s!!%s  %s\n' "$C_Y" "$C_0" "$*" >&2; }
die()  { printf '%sОшибка:%s %s\n' "$C_R" "$C_0" "$*" >&2; exit 1; }

usage() {
  cat <<USAGE
xmath — установщик (свободная альтернатива Mathcad 15)

Использование: install.sh [install|update|uninstall] [параметры]

Действия:
  install       установить (по умолчанию)
  update        получить свежие исходники, пересобрать и переустановить
  uninstall     удалить программу, ярлык, иконку и MIME-регистрацию

Параметры:
  --branch <имя>   ветка git (env XMATH_BRANCH, по умолчанию: main)
  --repo <url>     адрес репозитория (env XMATH_REPO,
                   по умолчанию: https://github.com/t1murjan/xmath.git)
  --prefix <dir>   каталог установки (по умолчанию: ~/.local)
  --from-local     собрать из каталога со скриптом, не клонируя
  --with-skill     установить AI-навык в ~/.claude/skills/xmath
  --force          пересобрать, даже если версия не изменилась
  --purge          при uninstall: удалить и каталог с исходниками
  -y, --yes        не задавать вопросов
  -h, --help       эта справка
USAGE
}

while [ $# -gt 0 ]; do
  case "$1" in
    install|update|uninstall) ACTION="$1" ;;
    --branch) [ $# -ge 2 ] || die "--branch требует значение"; BRANCH="$2"; shift ;;
    --repo)   [ $# -ge 2 ] || die "--repo требует значение"; REPO="$2"; shift ;;
    --prefix) [ $# -ge 2 ] || die "--prefix требует значение"; PREFIX="$2"; shift ;;
    --from-local) FROM_LOCAL=1 ;;
    --with-skill) WITH_SKILL=1 ;;
    --force) FORCE=1 ;;
    --purge) PURGE=1 ;;
    -y|--yes) ASSUME_YES=1 ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; die "неизвестный параметр: $1" ;;
  esac
  shift
done

BIN_DIR="$PREFIX/bin"
APP_DIR="$DATA_HOME/applications"
ICON_DIR="$DATA_HOME/icons/hicolor/scalable/apps"
MIME_DIR="$DATA_HOME/mime"
SKILL_DIR="$HOME/.claude/skills/xmath"

have() { command -v "$1" >/dev/null 2>&1; }

confirm() {
  # По умолчанию «да»; с --yes или без TTY сразу «да».
  [ "$ASSUME_YES" -eq 1 ] && return 0
  if [ ! -t 0 ]; then
    warn "Нет интерактивного терминала, продолжаю автоматически: $1"
    return 0
  fi
  local ans
  read -r -p "$1 [Y/n] " ans
  case "$ans" in n|N|no|No|NO|н|Н) return 1 ;; *) return 0 ;; esac
}

pkg_hint() {
  if have apt-get; then
    warn "Подсказка (apt): sudo apt install build-essential pkg-config git curl"
  elif have dnf; then
    warn "Подсказка (dnf): sudo dnf install gcc pkgconf-pkg-config git curl"
  elif have pacman; then
    warn "Подсказка (pacman): sudo pacman -S base-devel pkgconf git curl"
  elif have zypper; then
    warn "Подсказка (zypper): sudo zypper install gcc pkg-config git curl"
  fi
}

refresh_caches() {
  if have update-desktop-database; then update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true; fi
  if have update-mime-database; then update-mime-database "$MIME_DIR" >/dev/null 2>&1 || true; fi
  if have gtk-update-icon-cache; then gtk-update-icon-cache -q -t -f "$DATA_HOME/icons/hicolor" >/dev/null 2>&1 || true; fi
}

do_uninstall() {
  info "Удаление xmath"
  local b
  for b in "${BINS[@]}"; do
    if [ -e "$BIN_DIR/$b" ]; then
      rm -f "$BIN_DIR/$b"
      info "удалён $BIN_DIR/$b"
    fi
  done
  rm -f "$APP_DIR/xmath.desktop" "$ICON_DIR/xmath.svg" "$MIME_DIR/packages/xmath.xml"
  rm -f "$VERSION_FILE"
  refresh_caches
  if [ -d "$SKILL_DIR" ]; then
    warn "AI-навык оставлен: $SKILL_DIR (удалите вручную: rm -rf $SKILL_DIR)"
  fi
  if [ "$PURGE" -eq 1 ]; then
    rm -rf "$DATA_HOME/xmath"
    info "удалён каталог $DATA_HOME/xmath"
  else
    info "Исходники сохранены в $SRC_DIR (--purge удалит их)"
  fi
  info "Готово."
}

if [ "$ACTION" = "uninstall" ]; then
  do_uninstall
  exit 0
fi

# ---- проверка зависимостей ----
missing=()
if [ "$FROM_LOCAL" -eq 0 ]; then
  have git || missing+=("git")
  have curl || missing+=("curl")
fi
have cc || have gcc || have clang || missing+=("cc (C-компилятор/линкер)")
if [ "${#missing[@]}" -gt 0 ]; then
  warn "Не найдены: ${missing[*]}"
  pkg_hint
  die "установите недостающие пакеты и запустите скрипт снова"
fi

if ! have cargo && [ -x "$HOME/.cargo/bin/cargo" ]; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi
if ! have cargo; then
  warn "Rust (cargo) не найден."
  have curl || die "для установки Rust нужен curl"
  if confirm "Установить Rust через rustup (https://sh.rustup.rs)?"; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
    if [ -f "$HOME/.cargo/env" ]; then
      # shellcheck disable=SC1091
      . "$HOME/.cargo/env"
    fi
    have cargo || die "не удалось установить Rust"
  else
    die "для сборки нужен Rust: https://rustup.rs"
  fi
fi

# ---- исходники ----
if [ "$FROM_LOCAL" -eq 1 ]; then
  SCRIPT_PATH="${BASH_SOURCE[0]:-$0}"
  [ -f "$SCRIPT_PATH" ] || die "--from-local требует запуск из файла, а не через pipe"
  SRC_DIR="$(cd "$(dirname "$SCRIPT_PATH")" && pwd)"
  info "Сборка из локального каталога: $SRC_DIR"
else
  mkdir -p "$(dirname "$SRC_DIR")"
  if [ -d "$SRC_DIR/.git" ]; then
    info "Обновление исходников ($BRANCH)"
    git -C "$SRC_DIR" remote set-url origin "$REPO"
    git -C "$SRC_DIR" fetch --depth 1 origin "$BRANCH"
    git -C "$SRC_DIR" reset --hard FETCH_HEAD
  else
    info "Клонирование $REPO ($BRANCH)"
    rm -rf "$SRC_DIR"
    git clone --depth 1 --branch "$BRANCH" "$REPO" "$SRC_DIR"
  fi
fi

NEW_COMMIT="unknown"
if have git; then
  NEW_COMMIT="$(git -C "$SRC_DIR" rev-parse HEAD 2>/dev/null || echo unknown)"
fi
OLD_COMMIT=""
if [ -f "$VERSION_FILE" ]; then
  OLD_COMMIT="$(cat "$VERSION_FILE" 2>/dev/null || true)"
fi

if [ -n "$OLD_COMMIT" ]; then
  info "Версия: ${OLD_COMMIT:0:10} → ${NEW_COMMIT:0:10}"
  if [ "$OLD_COMMIT" = "$NEW_COMMIT" ] && [ "$NEW_COMMIT" != unknown ] && [ "$FORCE" -eq 0 ]; then
    if [ -x "$BIN_DIR/xmath-cli" ] || [ -x "$BIN_DIR/xmath" ]; then
      info "Уже установлена последняя версия (--force для пересборки)."
      exit 0
    fi
  fi
fi

# ---- сборка ----
info "Сборка (cargo build --release), это может занять несколько минут"
cd "$SRC_DIR"
if ! cargo build --release --locked; then
  warn "Сборка с --locked не удалась, пробую без него"
  cargo build --release || { pkg_hint; die "сборка не удалась"; }
fi

TARGET_DIR="${CARGO_TARGET_DIR:-$SRC_DIR/target}"
mkdir -p "$BIN_DIR"
installed=()
for b in "${BINS[@]}"; do
  if [ -f "$TARGET_DIR/release/$b" ]; then
    install -m 755 "$TARGET_DIR/release/$b" "$BIN_DIR/$b"
    installed+=("$b")
  fi
done
[ "${#installed[@]}" -gt 0 ] || die "не найдено ни одного собранного бинарника"
info "Установлено в $BIN_DIR: ${installed[*]}"

# ---- интеграция с рабочим столом ----
has_gui=0
has_mcp=0
for b in "${installed[@]}"; do
  [ "$b" = xmath ] && has_gui=1
  [ "$b" = xmath-mcp ] && has_mcp=1
done

if [ "$has_gui" -eq 1 ]; then
  info "Интеграция с рабочим столом"
  mkdir -p "$APP_DIR" "$ICON_DIR" "$MIME_DIR/packages"
  cat > "$APP_DIR/xmath.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=xmath
GenericName=Math worksheets
GenericName[ru]=Математические расчёты
Comment=Free Mathcad 15 alternative
Comment[ru]=Свободная альтернатива Mathcad 15: расчёты с единицами измерения, графики, матрицы
Exec=$BIN_DIR/xmath %f
Icon=xmath
Terminal=false
Categories=Education;Science;Math;
MimeType=application/x-xmath;application/x-mathcad-xmcd;
DESKTOP
  if [ -f "$SRC_DIR/assets/xmath.svg" ]; then
    install -m 644 "$SRC_DIR/assets/xmath.svg" "$ICON_DIR/xmath.svg"
  fi
  cat > "$MIME_DIR/packages/xmath.xml" <<'MIME'
<?xml version="1.0" encoding="UTF-8"?>
<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
  <mime-type type="application/x-xmath">
    <comment>xmath worksheet</comment>
    <comment xml:lang="ru">Рабочий лист xmath</comment>
    <glob pattern="*.xmath"/>
  </mime-type>
  <mime-type type="application/x-mathcad-xmcd">
    <comment>Mathcad 15 worksheet</comment>
    <comment xml:lang="ru">Рабочий лист Mathcad 15</comment>
    <glob pattern="*.xmcd"/>
  </mime-type>
</mime-info>
MIME
  refresh_caches
fi

mkdir -p "$(dirname "$VERSION_FILE")"
echo "$NEW_COMMIT" > "$VERSION_FILE"

# ---- AI-навык и MCP ----
if [ -d "$SRC_DIR/skills/xmath" ]; then
  if [ "$WITH_SKILL" -eq 1 ]; then
    mkdir -p "$(dirname "$SKILL_DIR")"
    rm -rf "$SKILL_DIR"
    cp -r "$SRC_DIR/skills/xmath" "$SKILL_DIR"
    info "AI-навык установлен: $SKILL_DIR"
  else
    info "AI-навык не установлен. Чтобы поставить: install.sh --with-skill"
  fi
fi
if [ "$has_mcp" -eq 1 ]; then
  info "Подключение MCP-сервера к Claude Code:"
  echo "    claude mcp add xmath -- $BIN_DIR/xmath-mcp"
fi

# ---- PATH ----
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *)
    warn "$BIN_DIR отсутствует в PATH. Добавьте в ~/.bashrc:"
    echo "    export PATH=\"$BIN_DIR:\$PATH\""
    ;;
esac

info "Готово! Запуск: xmath (или xmath-cli файл.xmath)"
