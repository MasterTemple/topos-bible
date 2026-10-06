#!/usr/bin/env bash
# Tests topos-bible.nvim in headless Neovim: once with the quickfix list, and once with Telescope
# if it is installed (PLUGINS is the folder with plenary.nvim and telescope.nvim; lazy.nvim's by
# default). Run from anywhere: tests/nvim/run.sh
set -euo pipefail
REPO=$(cd "$(dirname "$0")/../.." && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
cargo build -q --manifest-path "$REPO/Cargo.toml" -p topos-lsp -p topos-bible-cli

mkdir -p "$WORK/ws/.git" "$WORK/ws/My Notes" "$WORK/config" "$WORK/cache"
printf 'Jn 3:16 again\nand John 3:14-18\n' > "$WORK/ws/My Notes/a.md"
printf 'John 3, John 2; 3:16, Rom 8:28, Luke 2:1\n' > "$WORK/ws/b.txt"
printf 'See jn 3:16 here\n' > "$WORK/ws/open.txt"

expected='lsp topos hints true
diagnostic INFO reference John 3:16 (John.3.16)
search Bible references 7
query topos -g Gospels --exclude-book Luke 5
explicit Search "John 3:16" for explicit overlap 4
exact at cursor Search "John 3:16" for exact overlap 2
any Search "John 3" for any overlap 5
inside Search inside "John 3" 4
complete flags "Pauline Epistles"
complete ref 3:10|3:11|3:12
code action Search "John 3:16" for any overlap Search "John 3:16" for any overlap 5'

run() {
  rm -f "$WORK/out.txt"
  (cd "$WORK/ws" && env REPO="$REPO" WORK="$WORK" "$@" timeout 120 nvim --clean --headless -c "luafile $REPO/tests/nvim/test.lua" >/dev/null 2>&1) || true
  cat "$WORK/out.txt"
}

status=0
check() {
  local name=$1 output=$2
  if grep -q '^ERROR' <<<"$output" || [[ "$(head -n 11 <<<"$output")" != "$expected" ]]; then
    echo "FAIL ($name):"
    diff <(echo "$expected") <(head -n 11 <<<"$output") || true
    grep '^ERROR' <<<"$output" || true
    status=1
  else
    echo "ok ($name)"
  fi
}

check quickfix "$(run)"
plugins=${PLUGINS:-$HOME/.local/share/nvim/lazy}
if [[ -d "$plugins/telescope.nvim" && -d "$plugins/plenary.nvim" ]]; then
  output=$(run WITH_TELESCOPE=1 PLUGINS="$plugins")
  check telescope "$output"
  grep -q 'telescope ext Search "Rom 8:28" for explicit overlap 1' <<<"$output" || { echo "FAIL (telescope extension)"; status=1; }
else
  echo "skipped (telescope): not found in $plugins"
fi
exit $status
