#!/usr/bin/env python3
"""Check Rust event JNI calls against the compiled Java methods and arguments.

The Rust compiler cannot check JNI descriptor strings. A wrong descriptor can
silently bypass a cancellable event even when both languages compile. Inspect
every call in forward.rs, resolving literal method names passed through its
helpers, and compare to javap rather than a second hand-maintained method list.
Unsupported expressions fail the check instead of leaving an unverified call.
"""

import argparse
import dataclasses
import json
import pathlib
import re
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
SOURCE = REPO / "foton-plugin/src/forward.rs"
JAR = REPO / "plugin-api/build/foton-plugin-api.jar"


@dataclasses.dataclass(frozen=True)
class Token:
    text: str
    line: int


def tokens(source):
    """Only tokenize; Rust expressions are never evaluated by this checker."""
    pattern = re.compile(
        r'''"(?:\\.|[^"\\])*"|'(?:\\(?:u\{[0-9a-fA-F]+\}|x..|.)|[^'\\])'|//[^\n]*|/\*|[A-Za-z_][A-Za-z_0-9]*|\s+|.''',
        re.S)
    result = []
    offset = 0
    line = 1
    while offset < len(source):
        raw = re.match(r'b?r(#+)?"', source[offset:])
        if raw:
            terminator = '"' + (raw.group(1) or "")
            end = source.find(terminator, offset + raw.end())
            if end < 0:
                raise ValueError(f"line {line}: unterminated raw string")
            value = source[offset:end + len(terminator)]
            result.append(Token(value, line))
            offset += len(value)
            line += value.count("\n")
            continue
        match = pattern.match(source, offset)
        value = match.group()
        if value == "/*":
            depth = 1
            end = match.end()
            while depth:
                comment = re.search(r'/\*|\*/', source[end:])
                if comment is None:
                    raise ValueError(f"line {line}: unterminated comment")
                end += comment.end()
                depth += 1 if comment.group() == "/*" else -1
            value = source[offset:end]
        if not (value.isspace() or value.startswith(("//", "/*"))):
            result.append(Token(value, line))
        offset += len(value)
        line += value.count("\n")
    return result


def arguments(stream, opening):
    """Split a parenthesized call or array without splitting nested expressions."""
    pairs = {"(": ")", "[": "]", "{": "}"}
    stack = [pairs[stream[opening].text]]
    result = []
    start = opening + 1
    for index in range(start, len(stream)):
        value = stream[index].text
        if value in pairs:
            stack.append(pairs[value])
        elif value in pairs.values():
            if value != stack.pop():
                raise ValueError(f"line {stream[index].line}: unbalanced expression")
            if not stack:
                if index > start:
                    result.append(stream[start:index])
                return result, index
        elif value == "," and len(stack) == 1:
            result.append(stream[start:index])
            start = index + 1
    raise ValueError(f"line {stream[opening].line}: unterminated expression")


def literal(expression, constants):
    if len(expression) == 1:
        value = expression[0].text
        if value.startswith('"'):
            return json.loads(value)
        if value in constants:
            return constants[value]
    raise ValueError(f"line {expression[0].line}: expected a string literal or constant")


def parameter_names(stream, call, expression, constants):
    """Resolve a helper's method parameter from every direct caller, not a whitelist."""
    if len(expression) != 1:
        return {literal(expression, constants)}
    parameter = expression[0].text
    if parameter.startswith('"') or parameter in constants:
        return {literal(expression, constants)}
    owner = next((index for index in range(call - 1, -1, -1)
                  if stream[index].text == "fn"), None)
    if owner is None:
        raise ValueError(f"line {expression[0].line}: method is not statically resolvable")
    function = stream[owner + 1].text
    params, end = arguments(stream, owner + 2)
    slot = next((index for index, param in enumerate(params) if param[0].text == parameter), None)
    if slot is None:
        raise ValueError(f"line {expression[0].line}: {parameter} is not a helper parameter")
    body = next(index for index in range(end + 1, len(stream)) if stream[index].text == "{")
    _, end = arguments(stream, body)
    if sum(token.text == parameter for token in stream[body:end]) != 1:
        raise ValueError(f"line {expression[0].line}: helper must pass {parameter} directly without rewriting it")
    names = set()
    for index, token in enumerate(stream[:-1]):
        if token.text != function:
            continue
        if index > 0 and stream[index - 1].text == "fn":
            continue
        if stream[index + 1].text != "(":
            raise ValueError(f"line {token.line}: indirect reference to helper {function} cannot be audited")
        args, _ = arguments(stream, index + 1)
        names.add(literal(args[slot], constants))
    if not names:
        raise ValueError(f"line {expression[0].line}: no literal callers for {function}")
    return names


def descriptor_arguments(descriptor):
    primitives = dict(zip("ZBCSIJFD", ("Bool", "Byte", "Char", "Short", "Int", "Long", "Float", "Double")))
    if not descriptor.startswith("(") or ")" not in descriptor:
        raise ValueError(f"invalid method descriptor: {descriptor}")
    params = descriptor[1:descriptor.index(")")]
    result = []
    while params:
        match = re.match(r'\[*L[^;]+;|\[*[ZBCSIJFD]', params)
        if match is None:
            raise ValueError(f"invalid method descriptor: {descriptor}")
        value = match.group()
        result.append(primitives[value] if value in primitives else "Object")
        params = params[len(value):]
    return result


def value_arguments(expression):
    if len(expression) < 3 or [token.text for token in expression[:2]] != ["&", "["]:
        raise ValueError(f"line {expression[0].line}: JNI arguments must be an explicit slice")
    args, end = arguments(expression, 1)
    if end != len(expression) - 1:
        raise ValueError(f"line {expression[0].line}: unsupported JNI argument expression")
    result = []
    for arg in args:
        if len(arg) < 6 or [token.text for token in arg[:3]] != ["JValue", ":", ":"] or arg[4].text != "(":
            raise ValueError(f"line {arg[0].line}: expected JValue argument")
        result.append(arg[3].text)
    return result


@dataclasses.dataclass(frozen=True)
class Call:
    owner: str
    name: str
    descriptor: str
    line: int
    arguments: tuple


def calls(source):
    stream = tokens(source)
    constants = {}
    for index in range(len(stream) - 7):
        if stream[index].text != "const":
            continue
        declaration = [token.text for token in stream[index:index + 8]]
        if declaration[2:6] == [":", "&", "str", "="] and declaration[7] == ";":
            constants[declaration[1]] = json.loads(declaration[6])
    found = []
    for index, token in enumerate(stream):
        if token.text != "call_static_method":
            continue
        if index == 0 or index + 1 == len(stream) or stream[index - 1].text != "." or stream[index + 1].text != "(":
            raise ValueError(f"line {token.line}: unsupported JNI call syntax")
        args, _ = arguments(stream, index + 1)
        if len(args) != 4:
            raise ValueError(f"line {token.line}: expected four JNI call arguments")
        owner = literal(args[0], constants).replace("/", ".")
        descriptor = literal(args[2], constants)
        values = tuple(value_arguments(args[3]))
        for name in sorted(parameter_names(stream, index, args[1], constants)):
            found.append(Call(owner, name, descriptor, token.line, values))
    if not found:
        raise ValueError("no JNI calls found; refusing an empty audit")
    return found


def declarations(output):
    result = set()
    pending = None
    for line in output.splitlines():
        method = re.match(r'^\s*(?:public|protected|private) static\b.*?\b(\w+)\(.*\);$', line)
        if method:
            pending = method.group(1)
            continue
        descriptor = re.match(r'^\s*descriptor: (\S+)\s*$', line)
        if descriptor and pending is not None:
            result.add((pending, descriptor.group(1)))
        pending = None
    return result


def check(found, declared):
    errors = []
    for call in found:
        label = f"line {call.line}: {call.owner}.{call.name}{call.descriptor}"
        if (call.name, call.descriptor) not in declared.get(call.owner, set()):
            errors.append(f"{label} is not declared static in the compiled API")
        expected = tuple(descriptor_arguments(call.descriptor))
        if call.arguments != expected:
            errors.append(f"{label} expects {expected}, but Rust passes {call.arguments}")
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--quiet", action="store_true")
    parser.add_argument("--jar", type=pathlib.Path, default=JAR)
    args = parser.parse_args()
    try:
        if not args.jar.is_file():
            raise ValueError(f"{args.jar} is missing; build it with dev/build-plugin-api.sh first")
        found = calls(SOURCE.read_text(encoding="utf-8"))
        declared = {}
        for owner in sorted({call.owner for call in found}):
            result = subprocess.run(["javap", "-p", "-s", "-cp", str(args.jar), owner],
                                    capture_output=True, text=True, check=True)
            declared[owner] = declarations(result.stdout)
        errors = check(found, declared)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(f"event bridge audit failed: {error}", file=sys.stderr)
        if isinstance(error, subprocess.CalledProcessError) and error.stderr:
            print(error.stderr, file=sys.stderr, end="")
        return 1
    for error in errors:
        print(error, file=sys.stderr)
    if not args.quiet:
        print(f"{len(found)} event bridge call targets checked against {args.jar.name}; {len(errors)} errors")
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
