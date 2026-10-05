"""No-server regression for interrupted Via client ownership."""

import os
from pathlib import Path
import shlex
import shutil
import signal
import subprocess
import tempfile
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[1]


@unittest.skipUnless(os.name == "posix" and getattr(os, "geteuid", lambda: -1)() == 0,
                     "requires Linux root")
class ViaSignalTests(unittest.TestCase):
    def test_outer_signal_stops_slow_client_and_owned_server(self):
        if not shutil.which("unshare") or not shutil.which("ip") or not shutil.which("node"):
            self.skipTest("namespace and Node tools are required")
        namespace = subprocess.run(["unshare", "-n", "true"], capture_output=True)
        if namespace.returncode:
            self.skipTest("network namespaces are unavailable")

        with tempfile.TemporaryDirectory(prefix="foton-via-signal-") as directory:
            scratch = Path(directory)
            (scratch / "run" / "config").mkdir(parents=True)
            (scratch / "run" / "plugins").mkdir()
            (scratch / "bin").mkdir()
            fake_server = scratch / "fake-server.py"
            fake_server.write_text(textwrap.dedent("""\
                #!/usr/bin/env python3
                import os
                import select
                import socket
                import sys

                with open(os.path.join(os.environ["FOTON_VIA_PROBE_SCRATCH"], "server.pid"), "w") as output:
                    output.write(str(os.getpid()))
                with socket.socket() as listener:
                    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
                    listener.bind(("127.0.0.1", 25586))
                    listener.listen()
                    while True:
                        ready, _, _ = select.select([sys.stdin, listener], [], [], 0.1)
                        if sys.stdin in ready and sys.stdin.readline().strip() == "stop":
                            break
                        if listener in ready:
                            connection, _ = listener.accept()
                            connection.close()
                """), encoding="utf-8")
            fake_server.chmod(0o755)
            fake_node = scratch / "bin" / "node"
            fake_node.write_text(
                "#!/usr/bin/env bash\n"
                "if [[ ${1:-} == -e ]]; then\n"
                f"  exec {shlex.quote(shutil.which('node'))} \"$@\"\n"
                "fi\n"
                'printf \'%s\\n\' "$$" > "$FOTON_VIA_PROBE_SCRATCH/client.pid"\n'
                "exec sleep 120\n",
                encoding="utf-8",
            )
            fake_node.chmod(0o755)
            worker = scratch / "worker.sh"
            worker.write_text(textwrap.dedent(f"""\
                #!/usr/bin/env bash
                set -u
                repo={shlex.quote(str(ROOT))}
                scratch={shlex.quote(str(scratch))}
                temp_root={shlex.quote(str(scratch.parent))}
                isolated_pid=""
                export FOTON_VIA_PROBE_SCRATCH="$scratch"
                source <(sed -n "/^cleanup_outer() {{/,/^trap 'exit 143' TERM/p" "$repo/dev/via-test.sh")
                unshare -n bash -c 'ip link set lo up && exec bash "$@"' bash \
                  "$repo/dev/via-test.sh" --isolated "$scratch" {shlex.quote(str(fake_server))} /tmp 25586 /tmp &
                isolated_pid=$!
                for _ in $(seq 1 100); do
                  [[ -f "$scratch/client.pid" ]] && break
                  sleep 0.1
                done
                [[ -f "$scratch/client.pid" ]] || exit 1
                kill -TERM "$$"
                exit 0
                """), encoding="utf-8")
            worker.chmod(0o755)
            environment = os.environ.copy()
            environment["PATH"] = f"{scratch / 'bin'}:{environment['PATH']}"
            environment["FOTON_VIA_CLIENT_TIMEOUT_MS"] = "300000"
            try:
                result = subprocess.run(["bash", str(worker)], env=environment,
                                        capture_output=True, text=True, timeout=50)
                details = result.stdout + result.stderr
                self.assertEqual(143, result.returncode, details)
                self.assertIn("Via test evidence retained in", details)
                self.assertTrue((scratch / "server.log").is_file(), details)
                for name in ("server.pid", "client.pid"):
                    self.assertTrue((scratch / name).is_file(), details)
                    pid = int((scratch / name).read_text(encoding="utf-8"))
                    with self.assertRaises(ProcessLookupError, msg=f"{name}: {details}"):
                        os.kill(pid, 0)
            finally:
                for name in ("server.pid", "client.pid"):
                    path = scratch / name
                    if path.is_file():
                        try:
                            os.kill(int(path.read_text(encoding="utf-8")), signal.SIGKILL)
                        except ProcessLookupError:
                            pass


if __name__ == "__main__":
    unittest.main()
