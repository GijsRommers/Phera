#!/usr/bin/env python3
"""Reject unsafe tokens even in cfg-disabled Rust; ignore comments and literals."""

import pathlib
import re
import sys


def unsafe_offsets(source):
    offset = 0
    while offset < len(source):
        if source.startswith('//', offset):
            end = source.find('\n', offset)
            offset = len(source) if end == -1 else end
        elif source.startswith('/*', offset):
            depth = 1
            offset += 2
            while depth and offset < len(source):
                if source.startswith('/*', offset):
                    depth += 1
                    offset += 2
                elif source.startswith('*/', offset):
                    depth -= 1
                    offset += 2
                else:
                    offset += 1
        elif raw := re.match(r'(?:br|cr|r)(#*)"', source[offset:]):
            terminator = '"' + raw[1]
            end = source.find(terminator, offset + raw.end())
            if end == -1:
                raise ValueError('unterminated Rust raw string')
            offset = end + len(terminator)
        elif source[offset] == '"':
            offset += 1
            while offset < len(source):
                if source[offset] == '\\':
                    offset += 2
                elif source[offset] == '"':
                    offset += 1
                    break
                else:
                    offset += 1
        elif char := re.match(r"'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.)|[^'\\\n])'", source[offset:]):
            # A lifetime has no closing quote and must remain code.
            offset += char.end()
        elif identifier := re.match(r'\w+', source[offset:]):
            if identifier[0] == 'unsafe':
                yield offset
            offset += identifier.end()
        else:
            offset += 1


def main():
    failed = False
    for path in sorted(pathlib.Path('crates').rglob('*.rs')):
        source = path.read_text()
        for offset in unsafe_offsets(source):
            line = source.count('\n', 0, offset) + 1
            print(f'{path}:{line}: unsafe Rust is forbidden in Phera', file=sys.stderr)
            failed = True
    return int(failed)


if __name__ == '__main__':
    sys.exit(main())
