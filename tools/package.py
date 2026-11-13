#!/usr/bin/env python3
"""Pack Armory Lib into the file Horizon loads.

    python3 tools/package.py

Writes dist/armory_lib-<version>.hnmod (for horizon-native-mods/) and dist/SHA256SUMS.txt.
The zip is deterministic and in the layout Horizon's docs/api/mod-files.md describes:
horizon-native.json and mixins/armory_lib/ with only .rs and .toml files and the licence.
"""
import hashlib
import json
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DIST = ROOT / "dist"


def main():
    compat = json.loads((ROOT / "compatibility.json").read_text())
    bundle = next(p for p in (ROOT / "mixins").iterdir() if (p / "horizon-mixin.toml").is_file())
    mixin = tomllib.loads((bundle / "horizon-mixin.toml").read_text())
    mod_id, version = mixin["mod"]["id"], mixin["mod"]["version"]
    if mixin["compatibility"]["horizon_revision"] != compat["horizon_revision"]:
        raise SystemExit("horizon-mixin.toml and compatibility.json pin different Horizon commits")
    api = tomllib.loads((bundle / mixin["api"]["path"] / "Cargo.toml").read_text())
    if api["package"]["version"] != version:
        raise SystemExit("the api package and horizon-mixin.toml have different versions")
    manifest = {"schema": 1, "id": mod_id, "version": version, "game": compat["game"],
                "horizon": compat["horizon_revision"], "mixins": compat["horizon_mixins_revision"]}
    files = {"horizon-native.json": (json.dumps(manifest, indent=2, sort_keys=True) + "\n").encode()}
    for path in sorted(bundle.rglob("*")):
        if not path.is_file() or "target" in path.parts:
            continue
        if path.suffix in (".rs", ".toml") or path.name.startswith("LICENSE"):
            files[f"mixins/{mod_id}/{path.relative_to(bundle).as_posix()}"] = path.read_bytes()
    DIST.mkdir(exist_ok=True)
    out = DIST / f"{mod_id}-{version}.hnmod"
    with zipfile.ZipFile(out, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for name, content in sorted(files.items()):
            info = zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o100644 << 16
            archive.writestr(info, content)
    digest = hashlib.sha256(out.read_bytes()).hexdigest()
    (DIST / "SHA256SUMS.txt").write_text(f"{digest}  {out.name}\n")
    print(f"{out.relative_to(ROOT)}  ({len(files)} files, {out.stat().st_size} bytes)\n{digest}")


if __name__ == "__main__":
    main()
