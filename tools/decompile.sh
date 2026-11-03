#!/usr/bin/env bash
# Decompiles the user's own Terraria.exe into .local/ (gitignored) as a reading aid.
# Usage: tools/decompile.sh <dossier Terraria>
# Needs the .NET SDK and ilspycmd (dotnet tool install -g ilspycmd --version 8.2.0.7535).
set -euo pipefail
game=${1:?usage: tools/decompile.sh <dossier Terraria>}
root=$(cd "$(dirname "$0")/.." && pwd)
exe="$game/Terraria.exe"
[ -f "$exe" ] || { echo "$exe introuvable" >&2; exit 1; }
version=$(sha256sum "$exe" | cut -c1-12)
out="$root/.local/terraria/$version"
mkdir -p "$out/sources"
ilspy=${ILSPYCMD:-ilspycmd}
command -v "$ilspy" >/dev/null || ilspy="$HOME/.dotnet/tools/ilspycmd"
DOTNET_ROLL_FORWARD=Major "$ilspy" -p -o "$out/sources" -r "$game" "$exe" > "$out/ilspy.log" 2>&1
files=$(find "$out/sources" -name '*.cs' | wc -l)
echo "$files fichiers dans $out/sources"
