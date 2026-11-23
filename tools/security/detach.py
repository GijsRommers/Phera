#!/usr/bin/env python3
"""usage: detach.py <logfile> <pidfile> <command...>

Starts <command...> in a new session (setsid) with output appended to
<logfile>, detached from the tool call that launched it (whose process group
gets killed when the call ends), and writes the child's pid to <pidfile>.
Use it to run long fuzzing campaigns from a harness that reaps children:

    python3 tools/security/detach.py run.log run.pid bash -c \\
      "nice -n 15 ./target/release/phera-fuzz mutate --phera ../target/release/phera ..."
"""
import os
import sys

log, pidf, cmd = sys.argv[1], sys.argv[2], sys.argv[3:]
pid = os.fork()
if pid == 0:
    os.setsid()
    fd = os.open(log, os.O_WRONLY | os.O_CREAT | os.O_APPEND, 0o644)
    os.dup2(fd, 1)
    os.dup2(fd, 2)
    devnull = os.open(os.devnull, os.O_RDONLY)
    os.dup2(devnull, 0)
    os.execvp(cmd[0], cmd)
    os._exit(127)
with open(pidf, "w") as f:
    f.write(str(pid))
print(pid)
