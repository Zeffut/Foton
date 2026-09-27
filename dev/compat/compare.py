#!/usr/bin/env python3
"""Strict comparison and execution of pinned Paper/Foton observations.

The runner intentionally fails for actions it cannot drive. A startup ping is
useful evidence, but it never certifies a plugin's gameplay behavior.
"""

import argparse
import copy
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import time
import urllib.request

if os.name == "posix":
    import pty

ROOT = Path(__file__).resolve().parents[2]
BUILDS = Path(__file__).with_name("paper-builds.json")
USER_AGENT = "Foton-paper-oracle/0.1 (https://github.com/zeffut/Foton)"
MARKER = re.compile(r"ORACLE:([A-Za-z0-9_.:-]+)=(.+)")
# Expected failures require a reviewed, exact scenario and code change. The
# baseline has no allowed server/plugin errors; do not silently add patterns.
ALLOWED_LOG_ERRORS = ()


def validate_scenario(scenario, generated_fixture=False):
    for required in ("plugin", "server_version", "setup", "actions", "observations", "normalizers"):
        if required not in scenario:
            raise ValueError(f"scenario missing {required}")
    uncollected = set(scenario["observations"]) - {"startup_order", "required_markers"}
    if uncollected:
        raise ValueError(f"uncollected observation surface: {', '.join(sorted(uncollected))}")
    for action in scenario["actions"]:
        if action["type"] == "status_ping":
            continue
        if action["type"] != "console_command":
            raise ValueError(f"unsupported action {action['type']!r}; no behavioral observation can be made")
        if not isinstance(action.get("expect_marker"), str) or not action["expect_marker"]:
            raise ValueError("console_command requires expect_marker for an observed effect")
        if not isinstance(action.get("expect_value"), str):
            raise ValueError("console_command requires expect_value for an observed effect")
    if generated_fixture:
        if scenario["plugin"] != "OracleFixture":
            raise ValueError("generated fixture must be OracleFixture")
        return
    jars = scenario["setup"].get("jars", [])
    if not isinstance(jars, list) or not jars:
        raise ValueError("nonfixture scenario requires a checksum-pinned plugin JAR")
    for jar in jars:
        if "artifact" in jar:
            if not isinstance(jar["artifact"], str) or not jar["artifact"]:
                raise ValueError("artifact reference must be nonempty")
        elif not (isinstance(jar.get("path"), str) and
                  isinstance(jar.get("sha256"), str) and
                  re.fullmatch(r"[0-9a-fA-F]{64}", jar["sha256"])):
            raise ValueError("direct plugin JAR requires path and 64-character sha256")
    target = scenario["plugin"].split("@", 1)[0]
    if not target or target not in scenario["observations"].get("startup_order", []):
        raise ValueError(f"scenario requires target plugin lifecycle marker for {target!r}")


def find_unexpected_errors(lines):
    pattern = re.compile(
        r"\[(?:ERROR|SEVERE|FATAL)\]|\b(?:ERROR|SEVERE|FATAL):|"
        r"\b[A-Za-z_$][\w.$]*(?:Exception|Error)\b(?![-\w])|\bException in thread\b|"
        r"\b(?:plugin|server) failed to (?:load|enable|start|initialize)\b|"
        r"\bJNI\b.{0,80}\b(?:error|fail(?:ed|ure)?|exception)\b|"
        r"\b(?:error|fail(?:ed|ure)?|exception)\b.{0,80}\bJNI\b|"
        r"\b(?:decoder|encoder|reference[- ]count|refcnt)\b.{0,80}\b(?:error|fail(?:ed|ure)?|exception)\b|"
        r"\b(?:error|fail(?:ed|ure)?|exception)\b.{0,80}\b(?:decoder|encoder|reference[- ]count|refcnt)\b",
        re.IGNORECASE,
    )
    return [line for line in lines if pattern.search(line) and line not in ALLOWED_LOG_ERRORS]


def collect_log_observations(lines):
    collected = {"startup_order": [], "events": []}
    for line in lines:
        marker = MARKER.search(line)
        if not marker:
            continue
        key, value = marker.group(1), marker.group(2).strip()
        collected["events"].append({"key": key, "value": value})
        if key == "lifecycle.enabled":
            collected["startup_order"].append(value)
    return collected


def install_inputs(paths, plugins):
    names = [path.name.casefold() for path in paths]
    if len(names) != len(set(names)):
        raise ValueError("duplicate plugin filename in pinned input set")
    installed = []
    for source in paths:
        destination = plugins / source.name
        if destination.exists():
            raise ValueError(f"plugin destination already exists: {destination}")
        checksum = _sha256(source)
        shutil.copy2(source, destination)
        actual = _sha256(destination)
        if actual != checksum:
            raise RuntimeError(f"installed checksum mismatch: {destination}")
        installed.append({"name": destination.name, "sha256": actual})
    return installed


def marker_after(log, offset, key, value, timeout_seconds):
    deadline = time.monotonic() + timeout_seconds
    while True:
        with open(log, "r", encoding="utf-8", errors="replace") as stream:
            stream.seek(offset)
            lines = stream.read().splitlines()
        if any((match := MARKER.search(line)) and match.group(1) == key and
               match.group(2).strip() == value for line in lines):
            return True
        if time.monotonic() >= deadline:
            return False
        time.sleep(0.1)


def _pointer(data, path):
    if not path.startswith("/"):
        raise ValueError(f"normalizer path must be a JSON pointer: {path}")
    parts = [part.replace("~1", "/").replace("~0", "~") for part in path[1:].split("/")]
    owner = data
    for part in parts[:-1]:
        owner = owner[int(part)] if isinstance(owner, list) else owner[part]
    key = int(parts[-1]) if isinstance(owner, list) else parts[-1]
    return owner, key


def normalize_observation(data, rules):
    result = copy.deepcopy(data)
    for rule in rules:
        if set(rule) != {"path", "kind"}:
            raise ValueError(f"normalizer needs only path and kind: {rule}")
        owner, key = _pointer(result, rule["path"])
        value = owner[key]
        kind = rule["kind"]
        if kind == "port" and isinstance(value, int) and not isinstance(value, bool) and 0 < value < 65536:
            owner[key] = "<port>"
        elif kind == "uuid" and isinstance(value, str) and re.fullmatch(r"[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}", value):
            owner[key] = "<uuid>"
        elif kind == "timestamp" and isinstance(value, str) and re.fullmatch(r"\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?Z", value):
            owner[key] = "<timestamp>"
        else:
            raise ValueError(f"{rule['path']}: invalid {kind} normalizer value {value!r}")
    return result


def compare_observations(paper, foton, normalizers):
    paper = normalize_observation(paper, normalizers)
    foton = normalize_observation(foton, normalizers)
    differences = []

    def walk(left, right, path):
        if isinstance(left, dict) and isinstance(right, dict):
            for key in sorted(left.keys() | right.keys()):
                child = path + "/" + key.replace("~", "~0").replace("/", "~1")
                if key not in right:
                    differences.append(f"{child}: missing in Foton; Paper={left[key]!r}")
                elif key not in left:
                    differences.append(f"{child}: extra in Foton; Foton={right[key]!r}")
                else:
                    walk(left[key], right[key], child)
        elif isinstance(left, list) and isinstance(right, list):
            for index in range(max(len(left), len(right))):
                child = path + "/" + str(index)
                if index >= len(right):
                    differences.append(f"{child}: missing in Foton; Paper={left[index]!r}")
                elif index >= len(left):
                    differences.append(f"{child}: extra in Foton; Foton={right[index]!r}")
                else:
                    walk(left[index], right[index], child)
        elif type(left) is not type(right) or left != right:
            differences.append(f"{path or '/'}: Paper={left!r}; Foton={right!r}")

    walk(paper, foton, "")
    return differences


def _sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _paper_jar(version, cache):
    entry = json.loads(BUILDS.read_text(encoding="utf-8"))[version]
    target = cache / f"paper-{version}-{entry['build']}.jar"
    if target.exists() and _sha256(target) == entry["sha256"]:
        return target
    request = urllib.request.Request(entry["url"], headers={"User-Agent": USER_AGENT})
    temporary = target.with_suffix(".download")
    try:
        with urllib.request.urlopen(request, timeout=60) as response, open(temporary, "wb") as stream:
            shutil.copyfileobj(response, stream)
        if _sha256(temporary) != entry["sha256"]:
            raise RuntimeError(f"Paper {version} build {entry['build']} checksum mismatch")
        temporary.replace(target)
    finally:
        temporary.unlink(missing_ok=True)
    return target


def _varint(value):
    result = bytearray()
    while True:
        part = value & 0x7f
        value >>= 7
        result.append(part | (0x80 if value else 0))
        if not value:
            return bytes(result)


def _read_varint(stream):
    value = 0
    for shift in range(0, 35, 7):
        part = stream.recv(1)
        if not part:
            raise EOFError("status connection closed")
        value |= (part[0] & 0x7f) << shift
        if not part[0] & 0x80:
            return value
    raise ValueError("status packet varint too long")


def _status_ping(port, protocol):
    with socket.create_connection(("127.0.0.1", port), timeout=3) as stream:
        stream.settimeout(3)
        address = b"localhost"
        payload = b"\0" + _varint(protocol) + _varint(len(address)) + address + struct.pack(">H", port) + b"\1"
        stream.sendall(_varint(len(payload)) + payload + b"\1\0")
        _read_varint(stream)
        if _read_varint(stream) != 0:
            raise RuntimeError("unexpected status packet ID")
        length = _read_varint(stream)
        chunks = bytearray()
        while len(chunks) < length:
            chunk = stream.recv(length - len(chunks))
            if not chunk:
                raise EOFError("truncated status JSON")
            chunks.extend(chunk)
        return json.loads(chunks.decode("utf-8"))


def _verify_inputs(scenario):
    paths = []
    for item in scenario["setup"].get("jars", []):
        if "artifact" in item:
            corpus = json.loads((ROOT / "dev/compat/corpus.json").read_text(encoding="utf-8"))
            found = [row for row in corpus["artifacts"] if
                     f"{row['name']}@{row['version']}#{row['variant']}" == item["artifact"]]
            if len(found) != 1 or not found[0].get("local_path") or not found[0].get("sha256"):
                raise RuntimeError(f"unavailable pinned artifact {item['artifact']}")
            source = found[0]["local_path"]
            if os.name == "posix" and re.match(r"^[A-Za-z]:/", source):
                source = f"/mnt/{source[0].lower()}/{source[3:]}"
            path = Path(source).expanduser().resolve(strict=True)
            checksum = found[0]["sha256"]
        else:
            path = Path(item["path"]).expanduser().resolve(strict=True)
            checksum = item["sha256"]
        if _sha256(path) != checksum:
            raise RuntimeError(f"input checksum mismatch: {path}")
        paths.append(path)
    return paths


def _fixture_jar(work, api_jar, javac, release):
    src = work / "OracleFixture.java"
    src.write_text(
        "import org.bukkit.plugin.java.JavaPlugin;\n"
        "public final class OracleFixture extends JavaPlugin {\n"
        "  @Override public void onEnable() { getLogger().info(\"ORACLE:lifecycle.enabled=OracleFixture\"); }\n"
        "  @Override public void onDisable() { getLogger().info(\"ORACLE:lifecycle.disabled=OracleFixture\"); }\n"
        "}\n", encoding="utf-8")
    classes = work / "classes"
    classes.mkdir()
    subprocess.run([str(javac), "--release", str(release), "-cp", str(api_jar), "-d", str(classes), str(src)], check=True)
    plugin_dir = classes / "plugin.yml"
    plugin_dir.write_text("name: OracleFixture\nversion: 1.0\nmain: OracleFixture\napi-version: '1.21'\n", encoding="utf-8")
    jar = work / "OracleFixture.jar"
    import zipfile
    with zipfile.ZipFile(jar, "w") as archive:
        for file in classes.rglob("*"):
            if file.is_file():
                archive.write(file, file.relative_to(classes).as_posix())
    major = int.from_bytes((classes / "OracleFixture.class").read_bytes()[6:8], "big")
    if major != release + 44:
        raise RuntimeError(f"fixture class version {major}, expected {release + 44}")
    return jar, major


def _prepare_foton(world, port, seed):
    config = world / "config"
    config.mkdir()
    for name in ("config.toml", "worlds.toml", "groups.toml"):
        source = ROOT / "package-content" / name
        data = source.read_text(encoding="utf-8")
        if name == "config.toml":
            data = re.sub(r"(?m)^server_port = \d+$", f"server_port = {port}", data)
            data = re.sub(r"(?m)^online_mode = true$", "online_mode = false", data)
            data = re.sub(r"(?m)^encryption = true$", "encryption = false", data)
        if name == "worlds.toml":
            data = re.sub(r'(?m)^seed = ""$', f'seed = "{seed}"', data)
        (config / name).write_text(data, encoding="utf-8")
    shutil.copy2(ROOT / "package-content/favicon.png", config / "favicon.png")


def _prepare_paper(world, port, seed):
    (world / "eula.txt").write_text("eula=true\n", encoding="ascii")
    (world / "server.properties").write_text(
        f"server-port={port}\nserver-ip=127.0.0.1\nonline-mode=false\n"
        f"level-seed={seed}\nlevel-name=world\n"
        "enable-rcon=false\nmax-players=20\nview-distance=4\nsimulation-distance=4\n"
        "motd=Foton Paper oracle\n", encoding="ascii")


def _observe(server, scenario, work, jar, java, foton_bin, api_jar, java_home, libs, fixture_major):
    if os.name != "posix":
        raise RuntimeError("real server orchestration requires Linux/WSL or another POSIX host")
    world = work / server
    world.mkdir()
    plugins = world / "plugins"
    plugins.mkdir()
    inputs = _verify_inputs(scenario)
    if jar:
        inputs.append(jar)
    installed = install_inputs(inputs, plugins)
    with socket.socket() as reserved:
        reserved.bind(("127.0.0.1", 0))
        port = reserved.getsockname()[1]
    seed = scenario["setup"].get("seed", 8675309)
    if server == "paper":
        _prepare_paper(world, port, seed)
        command = [str(java), "-Xms256M", "-Xmx1G", "-jar", str(_paper_jar(scenario["server_version"], work / "downloads")), "nogui"]
        environment = os.environ.copy()
    else:
        _prepare_foton(world, port, seed)
        command = [str(foton_bin)]
        environment = os.environ.copy()
        environment.update({"FOTON_PLUGIN_DIRECTORY": str(plugins), "FOTON_JAVA_HOME": str(java_home),
                            "FOTON_PLUGIN_API_JAR": str(api_jar), "FOTON_PLUGIN_LIBRARY_DIRECTORY": str(libs)})
    log = world / "server.log"
    observations = {"startup_order": [], "events": [], "server": {}, "fixture_class_major": fixture_major,
                    "installed_inputs": installed, "command_results": []}
    forced_shutdown = False
    with open(log, "w", encoding="utf-8") as output:
        master, slave = pty.openpty()
        try:
            process = subprocess.Popen(command, cwd=world, env=environment, stdin=slave,
                                       stdout=output, stderr=subprocess.STDOUT, text=True)
        finally:
            os.close(slave)
        try:
            deadline = time.monotonic() + scenario["setup"].get("startup_timeout_seconds", 180)
            status = None
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise RuntimeError(f"{server} exited during startup ({process.returncode}); see {log}")
                try:
                    status = _status_ping(port, 776 if scenario["server_version"] == "26.2" else 774)
                    break
                except (OSError, ValueError, EOFError, json.JSONDecodeError):
                    time.sleep(1)
            if status is None:
                raise RuntimeError(f"{server} did not answer status ping; see {log}")
            expected_protocol = 776 if scenario["server_version"] == "26.2" else 774
            if status["version"]["protocol"] != expected_protocol:
                raise RuntimeError(f"{server} reported protocol {status['version']['protocol']}, expected {expected_protocol}; see {log}")
            observations["server"] = {"protocol": status["version"]["protocol"],
                                      "online": status["players"]["online"], "port": port}
            for action in scenario["actions"]:
                if action["type"] == "status_ping":
                    status = _status_ping(port, observations["server"]["protocol"])
                    observations["server"]["protocol"] = status["version"]["protocol"]
                elif action["type"] == "console_command":
                    offset = log.stat().st_size
                    os.write(master, (action["command"] + "\r").encode("utf-8"))
                    if not marker_after(log, offset, action["expect_marker"], action["expect_value"],
                                        action.get("timeout_seconds", 10)):
                        raise RuntimeError(f"{server} command {action['command']!r} lacked expected effect "
                                           f"{action['expect_marker']}={action['expect_value']}; see {log}")
                    observations["command_results"].append({"command": action["command"],
                                                            "marker": action["expect_marker"],
                                                            "value": action["expect_value"]})
                else:
                    raise RuntimeError(f"unsupported action {action['type']!r}; no behavioral observation was made")
        finally:
            if process.poll() is None:
                try:
                    os.write(master, b"stop\r")
                    process.wait(timeout=20)
                except (OSError, subprocess.TimeoutExpired):
                    forced_shutdown = True
                    process.terminate()
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
            os.close(master)
    if forced_shutdown or process.returncode != 0:
        raise RuntimeError(f"{server} did not stop cleanly (exit {process.returncode}); see {log}")
    lines = log.read_text(encoding="utf-8", errors="replace").splitlines()
    unexpected = find_unexpected_errors(lines)
    if unexpected:
        raise RuntimeError(f"{server} unexpected error/exception: {unexpected[0]}; see {log}")
    observations.update(collect_log_observations(lines))
    expected = scenario["observations"]
    for plugin in expected.get("startup_order", []):
        if plugin not in observations["startup_order"]:
            raise RuntimeError(f"{server} did not enable expected plugin {plugin!r}; see {log}")
    for marker in expected.get("required_markers", []):
        if not any(event["key"] == marker for event in observations["events"]):
            raise RuntimeError(f"{server} missing required marker {marker!r}; see {log}")
    return observations, log


def run(args):
    if args.scenario == "fixture":
        scenario = {"plugin": "OracleFixture", "server_version": args.version or "26.2",
                    "setup": {"seed": 8675309, "jars": []},
                    "actions": [{"type": "status_ping"}],
                    "observations": {"startup_order": ["OracleFixture"],
                                     "required_markers": ["lifecycle.enabled", "lifecycle.disabled"]},
                    "normalizers": [{"path": "/server/port", "kind": "port"}]}
    else:
        scenario = json.loads(Path(args.scenario).read_text(encoding="utf-8"))
    builds = json.loads(BUILDS.read_text(encoding="utf-8"))
    version = scenario["server_version"]
    if version not in builds:
        raise ValueError(f"un-pinned Paper version {version}")
    if args.fixture and scenario["plugin"] != "OracleFixture":
        raise ValueError("--fixture only accepts OracleFixture scenario")
    if args.version and args.scenario != "fixture":
        raise ValueError("--version is only supported for the built-in fixture")
    java = Path(args.java).resolve(strict=True)
    javac = Path(args.javac).resolve(strict=True)
    java_home = java.parent.parent
    expected_java = builds[version]["java"]
    release_line = subprocess.check_output([str(java), "-version"], stderr=subprocess.STDOUT, text=True).splitlines()[0]
    if not re.search(rf'\b{expected_java}(?:\.|\b)', release_line):
        raise RuntimeError(f"Paper {version} requires Java {expected_java}, got {release_line}")
    temp_base = Path(tempfile.gettempdir()).resolve()
    timestamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    work = Path(tempfile.mkdtemp(prefix=f"foton-paper-oracle-{timestamp}-", dir=temp_base))
    (work / "downloads").mkdir()
    print(f"oracle evidence: {work}", flush=True)
    try:
        api_jar = Path(args.api_jar).resolve(strict=True)
        fixture = None
        major = None
        if args.fixture:
            fixture, major = _fixture_jar(work, api_jar, javac, expected_java)
        foton_bin = Path(args.foton_bin).resolve(strict=True) if not args.paper_only else None
        libs = Path(args.libs).resolve(strict=True) if not args.paper_only else None
        inputs = _verify_inputs(scenario)
        if fixture:
            inputs.append(fixture)
        validate_scenario(scenario, generated_fixture=args.fixture)
        names = [path.name.casefold() for path in inputs]
        if len(names) != len(set(names)):
            raise ValueError("duplicate plugin filename in pinned input set")
        evidence = {
            "paper": {"version": version, **builds[version]},
            "java": release_line,
            "fixture_class_major": major,
            "plugin_inputs": [{"name": path.name, "sha256": _sha256(path)} for path in inputs],
            "foton_binary_sha256": _sha256(foton_bin) if foton_bin else None,
            "foton_api_jar_sha256": _sha256(api_jar),
        }
        (work / "run.json").write_text(json.dumps(evidence, indent=2, sort_keys=True), encoding="utf-8")
        paper, paper_log = _observe("paper", scenario, work, fixture, java, foton_bin, api_jar, java_home, libs, major)
        evidence["installed_inputs"] = {"paper": paper["installed_inputs"]}
        (work / "run.json").write_text(json.dumps(evidence, indent=2, sort_keys=True), encoding="utf-8")
        (work / "paper.json").write_text(json.dumps(paper, indent=2, sort_keys=True), encoding="utf-8")
        if args.paper_only:
            print(f"Paper-only reference passed; build {builds[version]['build']}; evidence {work}")
            return 0
        foton, foton_log = _observe("foton", scenario, work, fixture, java, foton_bin, api_jar, java_home, libs, major)
        evidence["installed_inputs"]["foton"] = foton["installed_inputs"]
        (work / "run.json").write_text(json.dumps(evidence, indent=2, sort_keys=True), encoding="utf-8")
        (work / "foton.json").write_text(json.dumps(foton, indent=2, sort_keys=True), encoding="utf-8")
        differences = compare_observations(paper, foton, scenario["normalizers"])
        if differences:
            (work / "diff.txt").write_text("\n".join(differences) + "\n", encoding="utf-8")
            print("\n".join(differences), file=sys.stderr)
            print(f"Paper log: {paper_log}\nFoton log: {foton_log}", file=sys.stderr)
            return 1
        print(f"comparison passed; build {builds[version]['build']}; evidence {work}")
        return 0
    except Exception as error:
        (work / "error.txt").write_text(f"{type(error).__name__}: {error}\n", encoding="utf-8")
        print(f"oracle failed: {error}; evidence {work}", file=sys.stderr)
        return 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    compare = subparsers.add_parser("compare", help="compare two observation JSON files")
    compare.add_argument("scenario")
    compare.add_argument("paper")
    compare.add_argument("foton")
    execute = subparsers.add_parser("run", help="run both actual servers in temporary worlds")
    execute.add_argument("scenario")
    execute.add_argument("--java", required=True)
    execute.add_argument("--javac", required=True)
    execute.add_argument("--foton-bin", required=True)
    execute.add_argument("--api-jar", required=True)
    execute.add_argument("--libs", required=True)
    execute.add_argument("--fixture", action="store_true")
    execute.add_argument("--paper-only", action="store_true", help="historical Java 21 reference, without Foton parity claim")
    execute.add_argument("--version", choices=["26.2", "1.21.11"])
    args = parser.parse_args()
    if args.command == "run":
        return run(args)
    scenario = json.loads(Path(args.scenario).read_text(encoding="utf-8"))
    validate_scenario(scenario)
    paper = json.loads(Path(args.paper).read_text(encoding="utf-8"))
    foton = json.loads(Path(args.foton).read_text(encoding="utf-8"))
    differences = compare_observations(paper, foton, scenario["normalizers"])
    if differences:
        print("\n".join(differences), file=sys.stderr)
        return 1
    print("observations match")
    return 0


if __name__ == "__main__":
    sys.exit(main())
