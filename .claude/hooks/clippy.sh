#!/bin/sh
# Stop: when Rust files or manifests changed in the working tree, runs clippy as CI
# does and sends its errors back to Claude (exit 2) so it fixes them before handing
# off. Skipped on the stop that follows such a fix, so it can never loop.
if grep -q '"stop_hook_active" *: *true'; then
    exit 0
fi
cd "$CLAUDE_PROJECT_DIR" || exit 0
if [ -z "$(git status --porcelain -- '*.rs' '*Cargo.toml' 'clippy.toml')" ]; then
    exit 0
fi
if ! output=$(cargo clippy --workspace --all-targets --color never -- -D warnings 2>&1); then
    echo "cargo clippy --workspace --all-targets -- -D warnings failed:" >&2
    echo "$output" | grep -E -A 12 '^(error|warning)' | head -150 >&2
    exit 2
fi
exit 0
