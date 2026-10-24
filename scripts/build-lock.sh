#!/bin/bash
# Run one command under a machine-wide exclusive lock, so several agents' cargo builds never run
# at the same time (18 GB of RAM; parallel rustc on egui/wgpu trees OOMs the box).
#
#   build-lock.sh <command> [args...]
#
# macOS ships no flock(1), so the lock is fcntl through /usr/bin/python3. `python3` on this
# machine is a broken venv shim that silently executes nothing, hence the absolute path.
#
# The lock is held only for the child's lifetime; SIGKILL of the child still releases it.
set -uo pipefail

LOCKFILE="${ARTCRAFT_BUILD_LOCKFILE:-/tmp/artcraft-build.lock}"
WAIT="${ARTCRAFT_BUILD_LOCK_WAIT:-5400}"   # seconds to wait for the lock before giving up

[ "$#" -ge 1 ] || { echo "usage: build-lock.sh <command> [args...]" >&2; exit 2; }

exec /usr/bin/python3 - "$LOCKFILE" "$WAIT" "$@" <<'PY'
import fcntl, os, subprocess, sys, time

lockfile, wait, argv = sys.argv[1], float(sys.argv[2]), sys.argv[3:]
fd = os.open(lockfile, os.O_CREAT | os.O_RDWR, 0o666)
deadline = time.time() + wait
announced = False
while True:
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        break
    except OSError:
        if time.time() > deadline:
            sys.stderr.write("build-lock: timed out waiting for %s after %ds\n" % (lockfile, wait))
            sys.exit(3)
        if not announced:
            sys.stderr.write("build-lock: waiting for %s (another build is running)\n" % lockfile)
            announced = True
        time.sleep(2)

try:
    rc = subprocess.call(argv)
except FileNotFoundError as e:
    sys.stderr.write("build-lock: %s\n" % e)
    rc = 127
finally:
    fcntl.flock(fd, fcntl.LOCK_UN)
    os.close(fd)
sys.exit(rc)
PY
