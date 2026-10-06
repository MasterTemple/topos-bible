"""Adds PyPI metadata to the Python package that `boltffi pack python` generates.

Run after generating (`boltffi pack python --no-build`) and before building
(`boltffi pack python --regenerate false`); setuptools merges setup.cfg with the generated
setup.py.
"""

import shutil
import sys
from pathlib import Path

root = Path(__file__).resolve().parent.parent
package = root / "dist" / "python"
if not (package / "setup.py").is_file():
    sys.exit(f"{package} has no setup.py; run `boltffi pack python --no-build` first")

shutil.copy(root / "packaging" / "README.md", package / "README.md")
shutil.copy(root.parent.parent / "LICENSE", package / "LICENSE")
(package / "setup.cfg").write_text(
    """[metadata]
description = Find, parse, and complete Bible references in text
long_description = file: README.md
long_description_content_type = text/markdown
license_expression = CC0-1.0
license_files = LICENSE
url = https://github.com/MasterTemple/topos
project_urls =
    Source = https://github.com/MasterTemple/topos
    Issues = https://github.com/MasterTemple/topos/issues
keywords = bible, scripture, verse, reference, parser, osis
classifiers =
    Development Status :: 3 - Alpha
    Intended Audience :: Developers
    Programming Language :: Python :: 3
    Programming Language :: Rust
    Topic :: Religion
    Topic :: Text Processing
"""
)
print(f"added metadata to {package}")
