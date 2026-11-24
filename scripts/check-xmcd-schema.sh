#!/usr/bin/env bash
# Validates the `.xmcd` files xmath writes against the XML schemas Mathcad 15 itself ships
# (worksheet30.xsd, which imports math30.xsd and provenance10.xsd).
#
# The schemas are part of Mathcad, not of xmath, so they are not in this repository. Point the
# script at a directory that holds them:
#
#   XMATH_XSD_DIR=/path/to/schema scripts/check-xmcd-schema.sh [file.xmcd ...]
#
# Without arguments it exports every example worksheet from `examples/` and checks the result.
# Needs `xmllint` (libxml2).
set -euo pipefail
here=$(cd "$(dirname "$0")/.." && pwd)
: "${XMATH_XSD_DIR:?set XMATH_XSD_DIR to the directory with worksheet30.xsd}"
bin=${XMATH_BIN:-$here/target/debug/xmath-cli}
tmp=$(mktemp -d "${TMPDIR:-/tmp}/xmath-schema-XXXXXX")
trap 'rm -rf "$tmp"' EXIT

files=("$@")
if [ ${#files[@]} -eq 0 ]; then
    for f in "$here"/examples/*.xmath; do
        name=$(basename "$f" .xmath)
        "$bin" "$f" --export-xmcd "$tmp/$name.xmcd" >/dev/null 2>&1 || echo "export failed: $f"
        files+=("$tmp/$name.xmcd")
    done
fi

bad=0
for f in "${files[@]}"; do
    if out=$(cd "$XMATH_XSD_DIR" && xmllint --noout --schema worksheet30.xsd "$f" 2>&1); then
        echo "ok   $(basename "$f")"
    else
        bad=$((bad + 1))
        echo "FAIL $(basename "$f")"
        printf '%s\n' "$out" | grep "validity error" | head -5 || printf '%s\n' "$out" | head -3
    fi
done
echo "${#files[@]} checked, $bad invalid"
[ "$bad" -eq 0 ]
