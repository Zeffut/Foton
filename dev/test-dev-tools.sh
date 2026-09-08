#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

python3 - "$ROOT" <<'PY'
import pathlib
import socket
import subprocess
import sys
import tempfile
import threading

root = pathlib.Path(sys.argv[1])
build_script = (root / "dev/build-plugin-api.sh").read_text()
helper = root / "dev/wait-tcp.py"

assert "-printf" not in build_script, "plugin API classpath still uses GNU find -printf"
assert 'for jar in "$REPO/plugin-api/lib/"*.jar' in build_script, "plugin API classpath is not built from flat jars"
tcp_probe_scripts = [
    path for path in (root / "dev").glob("*.sh") if path.name != "test-dev-tools.sh"
]
remaining_ss_probes = [
    str(path.relative_to(root))
    for path in tcp_probe_scripts
    if "ss -ltn" in path.read_text()
]
assert not remaining_ss_probes, f"remaining ss -ltn readiness probes: {remaining_ss_probes}"


def run_helper(host, port, timeout, pid=None):
    command = [sys.executable, str(helper), host, str(port), str(timeout)]
    if pid is not None:
        command.append(str(pid))
    return subprocess.run(command, capture_output=True, text=True)


with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
    listener.bind(("127.0.0.1", 0))
    listener.listen()
    port = listener.getsockname()[1]
    thread = threading.Thread(target=lambda: listener.accept()[0].close(), daemon=True)
    thread.start()
    result = run_helper("127.0.0.1", port, 2)
    assert result.returncode == 0, result.stderr

try:
    with socket.socket(socket.AF_INET6, socket.SOCK_STREAM) as listener:
        listener.bind(("::1", 0))
        listener.listen()
        port = listener.getsockname()[1]
        thread = threading.Thread(target=lambda: listener.accept()[0].close(), daemon=True)
        thread.start()
        result = run_helper("::1", port, 2)
        assert result.returncode == 0, result.stderr
except OSError:
    pass

result = run_helper("127.0.0.1", 1, 0.2)
assert result.returncode == 1, (result.returncode, result.stderr)
assert "timeout" in result.stderr.lower(), result.stderr

dead = subprocess.Popen(["sleep", "0.05"])
dead.wait()
result = run_helper("127.0.0.1", 1, 2, dead.pid)
assert result.returncode == 2, (result.returncode, result.stderr)
assert "process" in result.stderr.lower() and "died" in result.stderr.lower(), result.stderr

with tempfile.TemporaryDirectory() as directory:
    worktree = pathlib.Path(directory) / "worktree"
    subprocess.run(["git", "-C", str(root), "worktree", "add", "--detach", str(worktree), "HEAD"], check=True, capture_output=True)
    try:
        hook_path = subprocess.check_output(
            ["git", "-C", str(worktree), "rev-parse", "--git-path", "hooks/pre-commit"], text=True
        ).strip()
        hook = pathlib.Path(hook_path)
        hook.parent.mkdir(parents=True, exist_ok=True)
        hook.write_text("#!/bin/sh\nexit 0\n")
        result = subprocess.run(
            ["bash", str(worktree / "dev/doctor.sh")],
            cwd=worktree,
            capture_output=True,
            text=True,
            timeout=30,
        )
        assert "[ OK ] pre-commit hook installed" in result.stdout, result.stdout
    finally:
        subprocess.run(["git", "-C", str(root), "worktree", "remove", "--force", str(worktree)], check=True, capture_output=True)

print("developer tooling tests passed")
PY
