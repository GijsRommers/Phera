"""Compare PHPStan and Phera JSON by source file, line and diagnostic kind."""

import argparse
from collections import Counter
import json
from pathlib import Path


STAN_IDENTIFIERS = {
    "attribute.notFound": "class.notFound",
    "classConstant.notFound": "classConstant.notFound",
    "generics.notFound": "class.notFound",
    "interface.notFound": "class.notFound",
    "new.classNotFound": "class.notFound",
    "staticMethod.notFound": "method.notFound",
    "trait.notFound": "class.notFound",
}


def identity(root, path, diagnostic, aliases):
    path = (root / path).resolve()
    identifier = diagnostic.get("identifier") or "(no identifier)"
    return path.relative_to(root).as_posix(), diagnostic["line"], aliases.get(identifier, identifier)


def phpstan(root, report):
    if "error_details" in report:
        if sum(map(len, report["error_details"].values())) != report["errors"]:
            raise ValueError("PHPStan truncated its report. Rerun with --no-ansi outside agent mode.")
        groups = report["error_details"]
    else:
        if report["errors"]:
            raise ValueError(f"PHPStan failed: {report['errors']}")
        groups = {path: group["messages"] for path, group in report["files"].items()}
    return {
        identity(root, path, message, STAN_IDENTIFIERS)
        for path, messages in groups.items()
        for message in messages
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True, help="Directory used to run both tools")
    parser.add_argument("--details", type=Path, help="Write the matched and unmatched locations")
    parser.add_argument("phpstan", type=Path)
    parser.add_argument("phera", type=Path)
    args = parser.parse_args()
    root = args.root.resolve()
    stan = phpstan(root, json.loads(args.phpstan.read_text()))
    phera = {
        identity(root, message["file"], message, {})
        for message in json.loads(args.phera.read_text())["diagnostics"]
    }
    groups = {"both": stan & phera, "only PHPStan": stan - phera, "only Phera": phera - stan}
    totals = [Counter(identifier for _, _, identifier in items) for items in groups.values()]
    print("| Identifier | Both | Only PHPStan | Only Phera |")
    print("| --- | ---: | ---: | ---: |")
    for identifier in sorted(set().union(*totals)):
        counts = " | ".join(str(count[identifier]) for count in totals)
        print(f"| {identifier} | {counts} |")
    print("| Total | " + " | ".join(str(len(items)) for items in groups.values()) + " |")
    if args.details:
        args.details.write_text(json.dumps({label: sorted(items) for label, items in groups.items()}, indent=2))


if __name__ == "__main__":
    main()
