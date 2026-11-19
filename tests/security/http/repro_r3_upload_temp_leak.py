#!/usr/bin/env python3
"""R3-2: an uploaded file's temporary copy is not deleted when the request is
aborted by the request timeout.

`crates/runtime/src/lib.rs` deletes the unclaimed upload temp files only after
`vm.run()` returns. When `answer()` in crates/server/src/connection.rs aborts
the request task at `--request-timeout`, that code never runs and the
`UploadedFiles` state is dropped with the VM, so the `phpXXXXXX` file stays in
the temporary directory forever (mode 0600, owned by the server user). The
same happens when the handler task panics. A client that uploads a file to a
script that then sleeps past the timeout leaves one file per request on disk;
repeated, that fills the temporary filesystem (and on a small /tmp, the whole
machine).

Safe to run: one small upload, one 504, bounded by timeouts; the reproducer
removes the leaked file it finds at the end. The server runs with its own
private TMPDIR so only its files are counted.

Usage: repro_r3_upload_temp_leak.py   (starts its own server)
"""
import os
import shutil
import sys
import tempfile
import time

from r3 import Server, make_site, report, status_of


def php_files(directory):
    try:
        return {n for n in os.listdir(directory) if n.startswith("php")}
    except OSError:
        return set()


def main():
    tmpdir = tempfile.mkdtemp(prefix="phera-r3-tmp-")
    boundary = b"Bb"
    part = (b"--" + boundary + b"\r\n"
            b'Content-Disposition: form-data; name="f"; filename="a.txt"\r\n\r\n'
            b"LEAKED-UPLOAD-CONTENT\r\n--" + boundary + b"--\r\n")
    content_type = f"multipart/form-data; boundary={boundary.decode()}"

    site = make_site({
        "index.php": "<?php echo 'FRONT';",
        "slow.php": "<?php sleep((int)($_GET['s'] ?? 30)); echo 'slept';",
    })
    # --request-timeout 3: the script sleeps 30s, so the request is aborted.
    server = Server(site, args=["--request-timeout", "3"], env_extra={"TMPDIR": tmpdir})
    try:
        before = php_files(tmpdir)
        resp = server.request(
            "/slow.php?s=30", [("Content-Type", content_type), ("Content-Length", str(len(part)))],
            body=part, method="POST", read_timeout=15,
        )
        code = status_of(resp)[0]
        time.sleep(1.0)  # give any cleanup a chance to run
        leaked = php_files(tmpdir) - before
        evidence = (
            f"upload to a script that outlives --request-timeout 3: client got {code} "
            f"('the request took too long'); temp files created by the request and still "
            f"present afterwards: {sorted(leaked) or 'none'}"
        )
        vulnerable = code == 504 and bool(leaked)
        # Be a good citizen: remove what the server leaked.
        for name in leaked:
            try:
                os.unlink(os.path.join(tmpdir, name))
            except OSError:
                pass
        return report("upload temp file survives the request timeout", vulnerable, evidence)
    finally:
        server.stop()
        shutil.rmtree(site, ignore_errors=True)
        shutil.rmtree(tmpdir, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
