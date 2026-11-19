"""Audit the entire lockfile. Every ignored advisory must have a reason."""

from pathlib import Path
import json
import re
import subprocess
import sys


root = Path(__file__).resolve().parents[2]
with (root / ".github" / "advisory-ignores.json").open(encoding="utf-8") as ignore_file:
    ignored = json.load(ignore_file)

options = []
for advisory, reason in ignored.items():
    if not re.fullmatch(r"RUSTSEC-\d{4}-\d+", advisory):
        sys.exit(f"Invalid RustSec advisory ID: {advisory}")
    if not isinstance(reason, str) or not reason.strip():
        sys.exit(f"Missing reason for ignoring {advisory}")
    options.extend(["--ignore", advisory])

audit = ["cargo", "audit", "--deny", "warnings", "--no-yanked", *options]
sys.exit(subprocess.run([*audit, *sys.argv[1:]], cwd=root).returncode)
