#!/usr/bin/env python3
"""No branch lands between an `adrp` and the `add` the linker made of a GOT load.

A GOT load is `adrp xN, :got:sym` then `ldr xN, [xN, :got_lo12:sym]`. When
`sym` is local, lld rewrites the pair to `adrp xN, sym` + `add xN, xN, :lo12:sym`
and assumes the `adrp` always runs first. GCC does not promise that: it can
reuse an `adrp` from another block and branch straight to the `ldr`. After the
rewrite, that path adds `sym`'s page offset to a different page and reads a
stray word (llvm/llvm-project#138254).

That happened in aws-lc's `SHA512_Update`. The skipped path tested bit 6 of a
GOT slot instead of `OPENSSL_armcap_P`. In 0.16.3 the slot held a libc
address with that bit set, so a Raspberry Pi 4 ran `sha512su0` and died in
SIGILL (#21). In 0.17.0 it held zero, so the same defect passed by luck.
`.cargo/config.toml` links aarch64-gnu with `--no-relax`. This script proves
the release binary carries no such pair. For every `adrp xN` immediately
followed by `add xN, xN, #imm` whose `add` is also a branch target, it walks
back along every path into that branch to the last write of `xN`, and accepts
the pair only if each one is an `adrp` of the same page. Compilers do emit that
legitimately (musl's allocator has one); a stale page is the defect.

    python3 dev/check-arm64-got-relax.py target/aarch64-unknown-linux-gnu/release/foton

Reads GNU or LLVM objdump output (`OBJDUMP` picks the binary). Standard
library only, like the rest of `dev/`.
"""

import os
import re
import subprocess
import sys

INSTRUCTION = re.compile(r"^\s*([0-9a-f]+):\s+(\S+)\s*(.*)$")
LABEL = re.compile(r"^[0-9a-f]+ <.*>:$")
BRANCH_TARGET = re.compile(r"(?:0x)?([0-9a-f]+) <")
REGISTER = re.compile(r"^[xw]([0-9]+)$")
DIRECT_BRANCHES = {"b", "cbz", "cbnz", "tbz", "tbnz"}
NO_FALLTHROUGH = {"b", "br", "ret", "braa", "brab", "retaa", "retab", "eret"}
# Instructions whose first operand is read, not written.
READS_FIRST = {"cmp", "cmn", "tst", "ccmp", "ccmn", "cbz", "cbnz", "tbz", "tbnz", "prfm", "br"}
# A search that has not settled after this many instructions is reported, not passed.
SEARCH_LIMIT = 4096


def objdump_command(binary: str) -> list[str]:
    tool = os.environ.get("OBJDUMP", "objdump")
    return [tool, "-d", "--no-show-raw-insn", binary]


def register(operand: str) -> int | None:
    match = REGISTER.match(operand.strip())
    return int(match[1]) if match else None


def page(args: str) -> int | None:
    target = BRANCH_TARGET.search(args)
    return int(target[1], 16) if target else None


def writes(op: str, args: str, reg: int) -> bool:
    """Whether the instruction may change register `reg` (x or w view)."""
    if op in ("bl", "blr"):
        # The procedure call standard lets a callee clobber x0-x18 and x30.
        return reg <= 18 or reg == 30
    operands = [operand.strip() for operand in args.split(",")]
    base = re.search(r"\[\s*[xw]?([0-9]+|sp)", args)
    if base and base[1] == str(reg) and (args.rstrip().endswith("!") or re.search(r"\],\s*#", args)):
        return True
    if op.startswith(("st", "b.")) or op in READS_FIRST or op in NO_FALLTHROUGH:
        # Stores write nothing but the status register of an exclusive store.
        return op.startswith(("stxr", "stlxr", "stxp", "stlxp")) and register(operands[0]) == reg
    destinations = operands[:2] if op.startswith("ld") and "p" in op[2:] else operands[:1]
    return any(register(operand) == reg for operand in destinations)


def scan(lines) -> tuple[int, int, list[str]]:
    """Returns (instructions, adrp+add pairs, pairs some path enters with another value)."""
    code: list[tuple[int, str, str, str]] = []
    falls: dict[int, bool] = {}
    for line in lines:
        if LABEL.match(line.strip()):
            if code:
                falls[code[-1][0]] = False
            continue
        match = INSTRUCTION.match(line)
        if match:
            code.append((int(match[1], 16), match[2], match[3], line.strip()))
    index = {address: position for position, (address, *_rest) in enumerate(code)}
    sources: dict[int, list[int]] = {}
    for address, op, args, _line in code:
        if op in DIRECT_BRANCHES or op.startswith("b."):
            target = page(args)
            if target is not None:
                sources.setdefault(target, []).append(address)

    def predecessors(position: int) -> list[int]:
        found = list(sources.get(code[position][0], []))
        if position > 0:
            before, op = code[position - 1][0], code[position - 1][1]
            if op not in NO_FALLTHROUGH and falls.get(before, True) and code[position][0] - before == 4:
                found.append(before)
        return found

    def reaches_with_page(start: int, reg: int, expected: int) -> bool:
        """Every path into `start` (inclusive) last wrote `reg` with `adrp reg, expected`."""
        stack, seen = [start], set()
        while stack:
            address = stack.pop()
            if address in seen:
                continue
            seen.add(address)
            if len(seen) > SEARCH_LIMIT or address not in index:
                return False
            position = index[address]
            _address, op, args, _line = code[position]
            if writes(op, args, reg):
                if op != "adrp" or page(args) != expected:
                    return False
                continue
            previous = predecessors(position)
            if not previous:
                return False
            stack.extend(previous)
        return True

    pairs, hits = 0, []
    for position in range(1, len(code)):
        address, op, args, line = code[position]
        before, previous_op, previous_args, _previous_line = code[position - 1]
        if op != "add" or previous_op != "adrp" or address - before != 4:
            continue
        operands = [operand.strip() for operand in args.split(",")]
        destination = previous_args.split(",")[0].strip()
        reg = register(destination)
        if len(operands) < 3 or reg is None or not operands[0] == operands[1] == destination:
            continue
        pairs += 1
        expected = page(previous_args)
        for source in sources.get(address, []):
            if expected is None or not reaches_with_page(source, reg, expected):
                hits.append(line)
                break
    return len(code), pairs, hits


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__.strip().splitlines()[0], file=sys.stderr)
        print("usage: check-arm64-got-relax.py <aarch64 binary>", file=sys.stderr)
        return 2
    with subprocess.Popen(
        objdump_command(sys.argv[1]), stdout=subprocess.PIPE, text=True
    ) as process:
        assert process.stdout is not None
        count, pairs, hits = scan(process.stdout)
    if process.returncode != 0:
        print(f"objdump failed with exit code {process.returncode}", file=sys.stderr)
        return 1
    # An empty scan means objdump changed format or the file is not aarch64:
    # passing then would be a check that never looked.
    if count < 1000 or pairs == 0:
        print(
            f"read {count} instructions and {pairs} adrp+add pairs; "
            "not a disassembly this script understands",
            file=sys.stderr,
        )
        return 1
    if hits:
        print(f"{len(hits)} adrp+add pair(s) entered with another page:", file=sys.stderr)
        for hit in hits[:20]:
            print(f"  {hit}", file=sys.stderr)
        print(
            "the linker relaxed a GOT load another path reaches without its adrp; "
            "link with --no-relax (see .cargo/config.toml)",
            file=sys.stderr,
        )
        return 1
    print(f"ok: {pairs} adrp+add pairs, none entered with another page ({count} instructions)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
