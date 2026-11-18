#!/usr/bin/env python3
"""Convert php-src's rare_cp_bitvec.h to the detector bitmap; pass php-src as argv[1]."""
import re
import struct
import sys
from pathlib import Path

source = Path(sys.argv[1]) / 'ext/mbstring/rare_cp_bitvec.h'
text = source.read_text().split('static const', 1)[1]
words = [int(word, 16) for word in re.findall(r'0x[0-9a-fA-F]+', text)]
assert len(words) == 2048
output = Path(__file__).resolve().parent.parent / 'data/rare.bin'
output.write_bytes(b''.join(struct.pack('<I', word) for word in words))
