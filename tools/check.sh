#!/bin/sh
# Type-checks the library against Horizon's source, staged the way the launcher stages it.
#   HORIZON_SRC   a clean checkout of https://github.com/horizon-rust/horizon at the commit in
#                 compatibility.json (it is not changed)
#   HORIZON_STAGE where to write the staged copy (default: target/horizon-stage)
#   horizon-mixins must be on PATH, at the commit in compatibility.json
#   tools/check.sh [extra cargo arguments]
# What the launcher does (launcher/src/native_mods at that commit) and this repeats:
# it strips [api] and [dependencies] from a copy of the manifest, applies the bundle with
# horizon-mixins (the bridge module and the sunburn injection), copies mixins/armory_lib/api
# to native-api/armory_lib in the staged workspace and points its horizon-client dependency
# at crates/horizon-client.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
src=${HORIZON_SRC:?set HORIZON_SRC to a clean Horizon checkout}
stage=${HORIZON_STAGE:-$here/target/horizon-stage}
mods=$(mktemp -d)
trap 'rm -rf "$mods"' EXIT
cp -r "$here/mixins/armory_lib" "$mods/armory_lib"
python3 - "$mods/armory_lib/horizon-mixin.toml" <<'PY'
import re, sys
path = sys.argv[1]
text = open(path).read()
text = re.sub(r'(?m)^\[api\]\npath = "[^"]*"\n', '', text)
text = re.sub(r'(?m)^\[dependencies\]\n(?:[\w-]+ = .*\n)*', '', text)
open(path, 'w').write(text)
PY
rm -rf "$stage"
horizon-mixins apply --workspace "$src" --mods "$mods" --out "$stage" >/dev/null
dest="$stage/native-api/armory_lib"
mkdir -p "$dest"
cp -r "$here/mixins/armory_lib/api/." "$dest/"
python3 - "$stage" <<'PY'
import re, sys, pathlib
stage = pathlib.Path(sys.argv[1])
cargo = stage / "native-api/armory_lib/Cargo.toml"
cargo.write_text(re.sub(
    r'horizon-client = \{[^}]*\}',
    'horizon-client = { path = "../../crates/horizon-client", default-features = false }',
    cargo.read_text()))
root = stage / "Cargo.toml"
root.write_text(re.sub(r'(members\s*=\s*\[)', r'\1\n    "native-api/armory_lib",',
                       root.read_text(), count=1))
PY
cd "$stage"
RUSTFLAGS="${RUSTFLAGS:--D warnings}" cargo check -p armory_lib -p horizon-client --lib "$@"
