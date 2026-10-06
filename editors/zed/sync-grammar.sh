#!/usr/bin/env bash
# Regenerates editors/tree-sitter-rove from src/syntax.rs and points the Zed
# extension at the latest commit containing it.
#
#   editors/zed/sync-grammar.sh          regenerate the grammar
#   editors/zed/sync-grammar.sh --rev    also update `rev` in extension.toml
set -euo pipefail

root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
cd "$root"

cargo run -q --manifest-path editors/tree-sitter-rove/gen/Cargo.toml

if [[ "${1:-}" == "--rev" ]]; then
    if [[ -n "$(git status --porcelain -- editors/tree-sitter-rove)" ]]; then
        echo "editors/tree-sitter-rove has uncommitted changes; commit them first so Zed can check them out" >&2
        exit 1
    fi
    rev="$(git log -1 --format=%H -- editors/tree-sitter-rove)"
    sed -i "s/^rev = \".*\"/rev = \"$rev\"/" editors/zed/extension.toml
    echo "grammar rev -> $rev"
fi
