#!/usr/bin/env python3
"""Every native the plugin API declares is registered, and nothing else is.

`RegisterNatives` is all-or-nothing: hand the JVM one (name, signature) pair
that `foton.Native` does not declare and the whole call throws, so no plugin
loads at all. Hand it fewer than the class declares and the missing ones are
fine until a plugin calls one, which is an `UnsatisfiedLinkError` -- a crash,
not a recoverable failure, and one no test reaches unless a plugin happens to
use that method.

Both halves are mechanical facts: `javap` prints what the class declares, and
`foton-plugin/src/natives.rs` lists what is registered. Comparing them is the
only way either mistake is caught before a server starts.

    python3 dev/check-natives.py          # report and exit non-zero on a gap
    python3 dev/check-natives.py --quiet  # exit code only

Standard library only, like the rest of `dev/`.
"""

import argparse
import pathlib
import re
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
JAR = REPO / "plugin-api" / "build" / "foton-plugin-api.jar"
NATIVES_RS = REPO / "foton-plugin" / "src" / "natives.rs"
NATIVE_CLASS = "foton.Native"


def declared():
    """The (name, descriptor) of every native method on the class."""
    result = subprocess.run(
        ["javap", "-p", "-s", "-cp", str(JAR), NATIVE_CLASS],
        capture_output=True, text=True,
    )
    if result.returncode != 0:
        sys.stderr.write(result.stderr)
        raise SystemExit(
            f"javap could not read {NATIVE_CLASS} from {JAR.relative_to(REPO)};"
            " build the API jar first with dev/build-plugin-api.sh"
        )

    found = set()
    pending = None
    for line in result.stdout.splitlines():
        stripped = line.strip()
        match = re.match(r"^descriptor: (\S+)$", stripped)
        if match:
            if pending is not None:
                found.add((pending, match.group(1)))
            pending = None
            continue
        # `public static native java.lang.String serverName();`
        match = re.match(r"^.*\bnative\b.*?(\w+)\(.*\);$", stripped)
        pending = match.group(1) if match else None
    return found


def registered():
    """The (name, signature) of every method handed to RegisterNatives."""
    source = NATIVES_RS.read_text(encoding="utf-8")
    # method(\n  "name",\n  "signature",\n  rust_fn as *mut c_void,\n)
    pattern = re.compile(
        r'method\(\s*"([^"]+)"\s*,\s*"([^"]+)"\s*,', re.MULTILINE)
    return {(name, signature) for name, signature in pattern.findall(source)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--quiet", action="store_true",
                        help="exit code only, no report")
    args = parser.parse_args()

    on_class = declared()
    in_table = registered()

    # Registered but not declared: RegisterNatives throws, nothing loads.
    ghosts = sorted(in_table - on_class)
    # Declared but not registered: UnsatisfiedLinkError when a plugin calls it.
    orphans = sorted(on_class - in_table)

    if not args.quiet:
        print(f"{len(on_class)} natives declared by {NATIVE_CLASS}")
        print(f"{len(in_table)} registered by foton-plugin/src/natives.rs")
        if ghosts:
            print(f"\n{len(ghosts)} registered but NOT declared -- "
                  "RegisterNatives throws and no plugin loads:")
            for name, signature in ghosts:
                print(f"    {name}{signature}")
        if orphans:
            print(f"\n{len(orphans)} declared but NOT registered -- "
                  "UnsatisfiedLinkError when a plugin calls one:")
            for name, signature in orphans:
                print(f"    {name}{signature}")
        if not ghosts and not orphans:
            print("\nevery declared native is registered, and nothing else is")

    return 1 if ghosts or orphans else 0


if __name__ == "__main__":
    sys.exit(main())
