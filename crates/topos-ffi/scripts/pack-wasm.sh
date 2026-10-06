#!/usr/bin/env bash
# Builds the npm package (topos-bible) into dist/wasm/pkg, with its README and license.
set -euo pipefail
cd "$(dirname "$0")/.."
boltffi pack wasm --release
cp packaging/README.md dist/wasm/pkg/README.md
cp ../../LICENSE dist/wasm/pkg/LICENSE
