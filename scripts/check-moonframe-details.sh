#!/bin/sh
# Exercise the actual panel modules with a focused worktree; never load a user's profile.
set -eu
if [ "$#" -ne 2 ]; then
    echo "usage: $0 MOONFRAME_BINARY MOONFRAME_TOOLBOX_DIR" >&2
    exit 2
fi
root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
toolbox=$(CDPATH='' cd -- "$2" && pwd)
app="$root/crates/sheprd-gpui/moonframe/app"
mkdir -p "$root/target"
fixture=$(mktemp -d "$root/target/moonframe-details.XXXXXX")
trap 'rm -rf "$fixture"' EXIT
mkdir "$fixture/components"
ln -s "$toolbox"/components/* "$fixture/components/"
ln -sf "$app/components/sheprd-status-dot.luau" "$fixture/components/sheprd-status-dot.luau"
cp "$toolbox/style.luau" "$fixture/style.luau"
module() {
    printf 'local %s = (function()\n' "$1"
    cat "$app/$2"
    printf '\nend)()\n'
}
{
    module shell main.luau
    module status mods/status/main.luau
    module details mods/details/main.luau
    cat "$app/../tests/details.luau"
} > "$fixture/main.luau"
"$1" --no-user --check "$fixture"
