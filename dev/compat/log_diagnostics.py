#!/usr/bin/env python3
"""Shared, strict diagnostics for normalized server logs."""

import argparse
from pathlib import Path
import re
import sys


# Expected failures require a reviewed, exact scenario and code change.
ALLOWED_LOG_ERRORS = ()
ERROR_PATTERN = re.compile(
    r"\[(?:ERROR|SEVERE|FATAL)\]|\b(?:ERROR|SEVERE|FATAL):|"
    r"\b[A-Za-z_$][\w.$]*(?:Exception|Error)\b(?![-\w])|"
    r"\bException(?::| in thread\b)|"
    r"\b(?:plugin|server) failed to (?:load|enable|start|initialize)\b|"
    r"\b(?:failed to|could not|unable to) (?:load|enable|start|initialize) (?:plugin\b|\S+\bplugin\b)|"
    r"\[host\].{0,80}\b(?:ViaVersion|ViaBackwards)\b.{0,80}\bfailed\b|"
    r"\[host\].{0,80}\bfailed\b.{0,80}\b(?:ViaVersion|ViaBackwards)\b|"
    r"\bJNI\b.{0,80}\b(?:error|fail|failed|failure|exception)\b|"
    r"\b(?:error|fail|failed|failure|exception)\b.{0,80}\bJNI\b|"
    r"\b(?:decoder|encoder|reference[- ]count|refcnt|refcount)\b.{0,80}\b(?:error|fail|failed|failure|exception)\b|"
    r"\b(?:error|fail|failed|failure|exception)\b.{0,80}\b(?:decoder|encoder|reference[- ]count|refcnt|refcount)\b|"
    r"\bReferenceCountUtil\b|\brefCnt\b|\brefcount\b",
    re.IGNORECASE,
)


def is_unexpected_error(line):
    return ERROR_PATTERN.search(line) is not None and line not in ALLOWED_LOG_ERRORS


def find_unexpected_errors(lines):
    """Return offending lines, preserving the comparator's original shape."""
    return [line for line in lines if is_unexpected_error(line)]


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("log", type=Path, help="normalized server log")
    args = parser.parse_args(argv)
    try:
        lines = args.log.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeError) as error:
        print(f"Cannot read normalized log {args.log}: {error}", file=sys.stderr)
        return 2
    if not lines:
        print(f"Invalid empty normalized log: {args.log}", file=sys.stderr)
        return 2
    unexpected = [(number, line) for number, line in enumerate(lines, 1)
                  if is_unexpected_error(line)]
    for number, line in unexpected:
        print(f"{number}:{line}", file=sys.stderr)
    return 1 if unexpected else 0


if __name__ == "__main__":
    sys.exit(main())
