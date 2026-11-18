#!/usr/bin/env python3
"""Extract Phera's HTML entity data from php-src's generated html_tables.h.

Run: python3 tools/generate-html-tables.py /path/to/php-src/ext/standard/html_tables.h
"""
import re
import sys
from pathlib import Path

header = Path(sys.argv[1]).read_text()
output = (
    Path(__file__).resolve().parents[1]
    / "crates/extensions/standard/src/strings/html/tables"
)
string = r'"([#a-zA-Z0-9]+)"'
hex_number = r"(0x[0-9A-F]+)"
hex_or_zero = r"(0x[0-9A-F]+|0)"
charset_names = ("win1252", "iso885915", "win1251", "iso88595", "cp866", "macroman", "koi8r")


def tables(prefix):
    return re.findall(rf"\b{prefix}(\w+)\b(?:\[\])? = {{(.*?)}};", header, re.S)


def write(name, rows):
    (output / name).write_text("".join("\t".join(row) + "\n" for row in rows))


# Copy every name, including aliases, from PHP's inverse lookup buckets.
for document in ("html4", "html5"):
    names = []
    for _, body in tables(f"ht_bucket_{document}_"):
        names.extend(re.findall(rf"{string}, \d+, {hex_number}, {hex_or_zero}", body))
    assert len(names) >= (250 if document == "html4" else 2100)
    write(f"{document}-names.tsv", sorted(names))


# Stage 3 rows are in code point order, with PHP's selected spelling.
# A multi-codepoint row has an optional single-character name, then pairs.
for document in ("be_noapos", "be_apos", "html4", "html5"):
    pairs = dict(tables(f"multi_cp_{document}_"))
    rows = []
    for start, body in sorted(tables(f"stage3_table_{document}_")):
        entries = re.findall(r'\{[01], \{ \{(NULL|"[^"]+"|\(void \*\)\w+), \d+\} \} \}', body)
        assert len(entries) == 64
        for offset, entry in enumerate(entries):
            first = hex(int(start, 16) + offset)
            if entry.startswith('"'):
                rows.append((entry.strip('"'), first, "0"))
            elif entry != "NULL":
                key = entry.removeprefix(f"(void *)multi_cp_{document}_")
                leading, *trailing = pairs[key].splitlines()[1:]
                name = re.search(string, leading)
                if name:
                    rows.append((name[1], first, "0"))
                for line in trailing:
                    name, second = re.search(rf"{string},\s*{hex_number}", line).groups()
                    rows.append((name, first, second))
    assert len(rows) >= 5
    write(f"{document}-translation.tsv", rows)


# PHP uses different tables in each direction for some charsets. Preserve
# both tables, including the shared stage-2 rows of the byte-to-Unicode maps.
byte_groups = dict(tables("enc_to_uni_s2_"))
byte_maps = dict(tables("enc_to_uni_"))
reverse_maps = dict(tables("unimap_"))
for charset in charset_names:
    points = [
        point
        for group in re.findall(r"&enc_to_uni_s2_(\w+)", byte_maps[charset])
        for point in re.findall(hex_number, byte_groups[group])
    ]
    assert len(points) == 256
    write(f"{charset}-to-unicode.tsv", [(point,) for point in points])
    if charset in reverse_maps:
        write(
            f"{charset}-from-unicode.tsv",
            re.findall(rf"\{{ {hex_number}, {hex_number} \}}", reverse_maps[charset]),
        )
