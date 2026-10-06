#!/usr/bin/env bash
# Builds release wheels with PyPI metadata into dist/python/wheelhouse.
# Pass interpreters to build for several Pythons: scripts/pack-python.sh --python python3.12 ...
set -euo pipefail
cd "$(dirname "$0")/.."
rm -rf dist/python
boltffi generate python
python=$(command -v python3 || command -v python || echo /opt/python/cp312-cp312/bin/python)
"$python" scripts/python-metadata.py
boltffi pack python --release --regenerate false "$@"
