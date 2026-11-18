#!/usr/bin/env python3
"""Generate deprecations.tsv from the PHP stubs and the reference app's keep list.

Run from this directory:
    python3 generate-deprecations.py ~/php-src origin/PHP-8.5.12 > deprecations.tsv
"""

import argparse
from pathlib import Path
import re
import subprocess


ATTRIBUTE = re.compile(r"#\[\\Deprecated\((.*)\)\]")
STRING_ARGUMENT = r'''\b{}:\s*(['"])((?:\\.|(?!\1).)*)\1'''
NAMESPACE = re.compile(r"^namespace(?:\s+([\w\\]+))?\s*([;{])", re.MULTILINE)
CLASS = re.compile(r"\s*(?:(?:final|abstract|readonly) )*(?:class|interface|trait|enum) (\w+)")
FUNCTION = re.compile(r"\bfunction &?(\w+)\(")
CONSTANT = re.compile(r"\bconst (?:[\w|?\\]+ )?(\w+)\s*=")


def git(checkout, *args):
    return subprocess.check_output(["git", "-C", checkout, *args], text=True)


def argument(attribute, name):
    match = re.search(STRING_ARGUMENT.format(name), attribute)
    return re.sub(r'''\\([\\'"])''', r"\1", match[2]) if match else ""


def stub_items(stub):
    namespace, namespace_depth = "", 0
    class_name = pending = None
    depth = 0
    for line in stub.splitlines():
        if match := NAMESPACE.match(line):
            namespace = f"{match[1]}\\" if match[1] else ""
            namespace_depth = int(match[2] == "{")
        if depth == namespace_depth and (match := CLASS.match(line)):
            class_name = namespace + match[1]
        if match := ATTRIBUTE.search(line):
            pending = argument(match[1], "since"), argument(match[1], "message")
        elif pending is not None:
            if match := FUNCTION.search(line):
                yield ("method", class_name + "::" + match[1], *pending) if class_name else (
                    "function", namespace + match[1], *pending
                )
                pending = None
            elif match := CONSTANT.search(line):
                yield ("class-constant", class_name + "::" + match[1], *pending) if class_name else (
                    "constant", namespace + match[1], *pending
                )
                pending = None
        depth += line.count("{") - line.count("}")
        if depth <= namespace_depth and "}" in line:
            class_name = None
    if pending is not None:
        raise ValueError("Unmatched Deprecated attribute")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("checkout", type=Path)
    parser.add_argument("ref")
    options = parser.parse_args()
    data = Path(__file__).resolve().parent
    kept = {tuple(line.split("\t")) for line in (data / "deprecated-kept.tsv").read_text().splitlines()
            if line and not line.startswith("#")}
    # These are internal deprecated methods in phpredis 6.3.0, which emits
    # the standard method deprecation without a version or replacement text.
    redis = ("delete", "open", "popen", "slaveof", "sortAsc", "sortAscAlpha", "sortDesc", "sortDescAlpha")
    items = [("method", f"Redis::{name}", "", "") for name in redis]
    for path in git(options.checkout, "ls-tree", "-r", "--name-only", options.ref).splitlines():
        if path.endswith(".stub.php") and not path.startswith(("ext/zend_test/", "ext/dl_test/")):
            items.extend(stub_items(git(options.checkout, "show", f"{options.ref}:{path}")))
    names = {(kind, name) for kind, name, _, _ in items}
    if len(names) != len(items) or kept - names:
        raise ValueError(f"Duplicate stub entries or obsolete keep entries: {kept - names}")
    commit = git(options.checkout, "rev-parse", options.ref).strip()
    print(f"# PHP stubs {options.ref} ({commit}) and phpredis 6.3.0")
    print("# kind\tname\tsince\tmessage\tpolicy")
    for kind, name, since, message in sorted(items):
        policy = "keep" if (kind, name) in kept else "omit"
        print("\t".join((kind, name, since, message, policy)))


if __name__ == "__main__":
    main()
