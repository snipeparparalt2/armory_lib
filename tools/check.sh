#!/bin/sh
# Type-checks the library against Horizon's source, as the launcher links it.
#   HORIZON_SRC  a checkout of https://github.com/horizon-rust/horizon at the commit in
#                compatibility.json (it is changed: the library is added as a workspace member)
#   tools/check.sh [extra cargo arguments]
# The launcher does the same when it builds: it copies mixins/armory_lib/api to
# native-api/armory_lib in the game's workspace and points its horizon-client dependency at
# crates/horizon-client.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
src=${HORIZON_SRC:?set HORIZON_SRC to a Horizon checkout}
dest="$src/native-api/armory_lib"
rm -rf "$dest"
mkdir -p "$dest"
cp -r "$here/mixins/armory_lib/api/." "$dest/"
python3 - "$src" <<'PY'
import re, sys, pathlib
src = pathlib.Path(sys.argv[1])
cargo = src / "native-api/armory_lib/Cargo.toml"
text = re.sub(r'horizon-client = \{[^}]*\}',
              'horizon-client = { path = "../../crates/horizon-client", default-features = false }',
              cargo.read_text())
cargo.write_text(text)
root = src / "Cargo.toml"
text = root.read_text()
if '"native-api/armory_lib"' not in text:
    text = re.sub(r'(members\s*=\s*\[)', r'\1\n    "native-api/armory_lib",', text, count=1)
    root.write_text(text)
PY
cd "$src"
RUSTFLAGS="${RUSTFLAGS:--D warnings}" cargo check -p armory_lib -p horizon-client --lib "$@"
