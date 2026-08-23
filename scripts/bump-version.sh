#!/usr/bin/env bash
# Sube la versión de la app en los TRES lugares donde vive, que tienen que
# coincidir entre sí y con el tag de git:
#
#   package.json          → la lee Vite y termina en APP_VERSION (src/lib/version.ts)
#   src-tauri/tauri.conf.json → nombra los instaladores (DiskDex_0.2.0_x64.dmg)
#   src-tauri/Cargo.toml  → la versión del crate
#
# El workflow de release aborta si alguna no coincide con el tag, así que esto
# existe para no tener que acordarse de las tres.
#
# Uso:
#   ./scripts/bump-version.sh 0.2.0
#   git commit -am "chore: v0.2.0" && git tag v0.2.0 && git push --follow-tags
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NEW="${1:-}"

if ! [[ "$NEW" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Uso: $0 <x.y.z>   (ej: $0 0.2.0)" >&2
  exit 1
fi

CUR="$(node -p "require('$ROOT/package.json').version")"
echo "$CUR → $NEW"

# package.json y tauri.conf.json: se editan con node para no romper el JSON con
# sed (y para tocar SOLO la clave de nivel raíz, no cualquier "version" anidada
# que aparezca más abajo en el archivo).
node -e '
  const fs = require("fs");
  const [file, version] = process.argv.slice(1);
  const raw = fs.readFileSync(file, "utf8");
  const json = JSON.parse(raw);
  json.version = version;
  // Preservar el newline final: si no, cada bump ensucia el diff.
  fs.writeFileSync(file, JSON.stringify(json, null, 2) + "\n");
' "$ROOT/package.json" "$NEW"

node -e '
  const fs = require("fs");
  const [file, version] = process.argv.slice(1);
  const json = JSON.parse(fs.readFileSync(file, "utf8"));
  json.version = version;
  fs.writeFileSync(file, JSON.stringify(json, null, 2) + "\n");
' "$ROOT/src-tauri/tauri.conf.json" "$NEW"

# Cargo.toml: solo la PRIMERA línea `version = ...`, que es la de [package].
# Las de las dependencias más abajo no se tocan.
perl -0pi -e 's/^version = "[^"]*"/version = "'"$NEW"'"/m if !$done++' "$ROOT/src-tauri/Cargo.toml"

# Cargo.lock guarda la versión del propio crate: sin esto queda desfasado y el
# build lo reescribe, ensuciando el árbol en pleno release.
( cd "$ROOT/src-tauri" && cargo update --workspace --offline >/dev/null 2>&1 ) || \
  ( cd "$ROOT/src-tauri" && cargo update --workspace >/dev/null 2>&1 ) || \
  echo "  ⚠ no se pudo actualizar Cargo.lock; corré 'cargo check' antes de commitear"

echo
echo "Quedó:"
printf '  package.json      %s\n' "$(node -p "require('$ROOT/package.json').version")"
printf '  tauri.conf.json   %s\n' "$(node -p "require('$ROOT/src-tauri/tauri.conf.json').version")"
printf '  Cargo.toml        %s\n' "$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/src-tauri/Cargo.toml" | head -1)"
echo
echo "Ahora:  git commit -am \"chore: v$NEW\" && git tag v$NEW && git push --follow-tags"
