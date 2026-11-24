#!/bin/sh
# End to end check of the command line tool on a scratch workspace.
set -eu
BIN="${1:-./target/debug/notecraft}"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
ok() { echo "ok   $1"; }
fail() { echo "FAIL $1"; exit 1; }

"$BIN" version >/dev/null && ok version
mkdir -p "$T/md/assets"
printf '# Hello\n\nText with $x$ and a [link](https://example.org).\n\n```mermaid\ngraph TD\n A-->B\n```\n\n$$\nx^2=4\n$$\n' > "$T/md/note.md"
"$BIN" import "$T/md" "$T/ws" >/dev/null && ok import
[ -d "$T/ws" ] || fail "import created nothing"
"$BIN" reindex "$T/ws" >/dev/null && ok reindex
"$BIN" search "$T/ws" Hello | grep -qi hello && ok search
"$BIN" import "$T/md" "$T/ws2" >/dev/null && ok "second import"
"$BIN" search "$T/ws2" Hello | grep -qi hello && ok "search imported"
"$BIN" export "$T/ws" "$T/out" >/dev/null && ok export
[ -n "$(ls "$T/out")" ] || fail "export empty"
"$BIN" export "$T/ws2" "$T/out2" >/dev/null
grep -rq 'mermaid' "$T/out2" && ok "mermaid survives import/export"
grep -rq 'x^2=4' "$T/out2" && ok "equation survives import/export"
"$BIN" snapshots "$T/ws" >/dev/null && ok snapshots
"$BIN" site "$T/ws" "$T/site" >/dev/null && ok site
[ -f "$T/site/index.html" ] && ok "site has an index" || fail "site incomplete"
grep -q 'class="crumbs"' "$T"/site/*.html && ok "site pages have a breadcrumb"
"$BIN" site /nonexistent/path "$T/x" >/dev/null 2>&1 && fail "site on a bad path accepted" || ok "site bad path rejected"
"$BIN" bench "$T/bench" 300 300 2>&1 | tail -5
"$BIN" nonsense >/dev/null 2>&1 && fail "unknown command accepted" || ok "unknown command rejected"
"$BIN" search /nonexistent/path foo >/dev/null 2>&1 && fail "bad path accepted" || ok "bad path rejected"
echo "all good"
