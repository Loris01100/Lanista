#!/bin/sh
# PostToolUse (Write|Edit): formats the workspace after Claude edits a Rust file.
# The tool call arrives as JSON on stdin; only its file_path matters.
if grep -q '"file_path" *: *"[^"]*\.rs"'; then
    cd "$CLAUDE_PROJECT_DIR" || exit 0
    cargo fmt --all >/dev/null 2>&1
fi
exit 0
