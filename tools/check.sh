#!/bin/sh
# Type-checks the library against Horizon's source, the way the launcher builds it.
#   HORIZON_SRC   a clean checkout of https://github.com/horizon-rust/horizon at the commit in
#                 compatibility.json (it is not changed)
#   HORIZON_STAGE where to write the staged copy (default: target/horizon-stage)
#   horizon-mixins must be on PATH, at the commit in compatibility.json
#   tools/check.sh [extra cargo arguments]
# Armory Lib is a plain mixin bundle, so the launcher applies it with horizon-mixins and
# builds the game's own package, horizon-client; this does the same and checks it with
# warnings denied.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
src=${HORIZON_SRC:?set HORIZON_SRC to a clean Horizon checkout}
stage=${HORIZON_STAGE:-$here/target/horizon-stage}
rm -rf "$stage"
horizon-mixins apply --workspace "$src" --mods "$here/mixins" --out "$stage" >/dev/null
cd "$stage"
RUSTFLAGS="${RUSTFLAGS:--D warnings}" cargo check -p horizon-client --bin horizon-client "$@"
