"""Import PHP timelib's public geography, abbreviation and explicit transition data."""
import re
import struct
import sys
from pathlib import Path

lib = Path(sys.argv[1]) / "ext/date/lib"
out = Path(__file__).resolve().parents[1] / "data"
header = (lib / "timezonedb.h").read_text().split("#else")[0]
index, payload = header.split("timelib_timezone_db_data_builtin", 1)
data = bytes.fromhex("".join(re.findall(r"0x([0-9A-Fa-f]{2})\b", payload)))
catalog = []
transitions = bytearray()
for name, pos in re.findall(r'"([^"]+)"\s*, 0x([0-9a-fA-F]+)', index):
    start = int(pos, 16)
    header_start = data.index(b"TZif", start + 20)
    _, _, leap, count, types, chars = struct.unpack_from(">6I", data, header_start + 20)
    assert leap == 0
    times = header_start + 44
    first = len(transitions)
    transitions.extend(data[times:times + 8 * count])
    location = data.index(b"\n", data.index(b"\n", times + 9 * count + 6 * types + chars) + 1) + 1
    lat, lon, length = struct.unpack_from(">III", data, location)
    comments = data[location + 12:location + 12 + length].decode()
    catalog.append([name, data[start + 4], data[start + 5:start + 7].decode(),
                    lat, lon, first, count, comments])
(out / "locations.tsv").write_text("".join("\t".join(map(str, row)) + "\n" for row in catalog))
(out / "transitions.bin").write_bytes(transitions)
for source, target in [("timezonemap", "zone-abbreviations"), ("fallbackmap", "zone-fallbacks")]:
    rows = re.findall(r'{ "(\w+)",\s*(\d),\s*(-?\d+)( \* 60)?,\s*("[^"]*"|NULL)',
                      (lib / (source + ".h")).read_text())
    (out / (target + ".tsv")).write_text("".join(
        f"{name}\t{dst}\t{int(offset) * (60 if minutes else 1)}\t"
        + (zone.strip('"') if zone != "NULL" else "") + "\n"
        for name, dst, offset, minutes, zone in rows))
