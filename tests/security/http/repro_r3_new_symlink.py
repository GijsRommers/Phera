#!/usr/bin/env python3
"""R3-5: `phera new` follows an existing symbolic link as its target.

`refuse_existing_files` in crates/cli/src/new.rs does `fs::read_dir(directory)`,
which follows symlinks, and only refuses when the target is non-empty. So
`phera new link` where `link -> /some/empty/dir` writes the new project into
/some/empty/dir. A setup that pre-creates `shop -> ~/.ssh` (or any empty
directory the user should not get files in) makes `phera new shop` write
README.md, composer.json, public/ and src/ there. Composer's create-project
also writes through the symlink.

Argument injection into Composer is NOT possible through the CLI: the argument
parser rejects any argument starting with `-` ("Unknown option"), and the
project name's basename is validated to [A-Za-z0-9._-]; a name like
`x/--prefer-dist/y` is passed to Composer as one positional path, not as an
option. (Verified with a fake `composer` on PATH that logs its argv.)

Safe to run: everything happens in a private temp dir; the reproducer removes
it. Bounded, no network.

Usage: repro_r3_new_symlink.py   (uses target/release/phera or $PHERA_BIN)
"""
import os
import shutil
import subprocess
import sys
import tempfile

from r3 import BIN, report


def main():
    work = tempfile.mkdtemp(prefix="phera-r3-new-")
    try:
        target = os.path.join(work, "victim-dir")
        os.makedirs(target)
        link = os.path.join(work, "shop")
        os.symlink(target, link)

        proc = subprocess.run(
            [BIN, "new", "shop", "--template", "plain", "--no-install"],
            cwd=work, capture_output=True, text=True, timeout=60,
            env={**os.environ, "PATH": os.environ["PATH"]},
        )
        landed = sorted(os.listdir(target)) if os.path.isdir(target) else []
        evidence = (
            f"phera new shop (exit {proc.returncode}) with shop -> {target} (empty dir): "
            f"the project files landed in the symlink target: {landed}"
        )
        vulnerable = proc.returncode == 0 and bool(landed)
        return report("phera new writes through an existing symlink", vulnerable, evidence)
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
