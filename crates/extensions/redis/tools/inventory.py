"""Writes the method inventory in README.md: for each of phpredis 6.3.0's
methods, which comparison scripts call it. Run from anywhere:

    python3 crates/extensions/redis/tools/inventory.py

A method counts as compared when a script other than 175_signatures.php
calls it (175 calls every method with wrong arguments only), or when one of
the Rust tests' inline PHP scripts does. A script that calls `$r->$method()`
counts every method it names in a string; so does `$r->{mode}()` in a Rust test.
"""
import json
import pathlib
import re

root = pathlib.Path(__file__).resolve().parents[1]
methods = list(json.loads((root / 'tools/phpredis-6.3.0.json').read_text())['methods'])
known = {name.lower(): name for name in methods}

unimplemented = set('failover gettransferredbytes cleartransferredbytes migrate vadd vsim vcard vdim '
                    'vinfo vismember vemb vrandmember vrange vrem vsetattr vgetattr vlinks'.split())
not_run = {name: 'Not run: it would change the shared test server.'
           for name in ['save', 'bgsave', 'bgrewriteaof', 'flushall']}
notes = {
    '__construct': 'The `ssl`, `backoff`, `retryInterval` and `maxRetries` options are checked, not used.',
    'connect': 'TCP, Unix sockets and `tls://` (rustls); no SSL context options.',
    'pconnect': 'Pooled like connect; the persistent id gets sockets of its own.',
    'reset': 'In pipeline mode phpredis raises a fatal error; Phera queues the command.',
    'expiremember': 'A KeyDB command; Redis answers with an error, compared as such.',
    'expirememberat': 'A KeyDB command; Redis answers with an error, compared as such.',
    'delifeq': 'A Valkey command; Redis 8.4 answers with an error, compared as such.',
    'setoption': 'Serializers NONE and JSON only; no compression.',
    'getoption': 'Retry, backoff and keepalive options are not kept.',
    'flushdb': 'Only on database 12, which belongs to its script.',
}
for name in ['_serialize', '_unserialize', '_pack', '_unpack', '_compress', '_uncompress']:
    notes[name] = 'Serializers NONE and JSON only; compression NONE only.'

sources = sorted((root / 'tests/scripts').glob('*.php')) + [root / 'tests/support.php'] + sorted((root / 'tests').glob('*.rs'))
callers = {name.lower(): [] for name in methods}
for source in sources:
    text = source.read_text()
    called = {match.lower() for match in re.findall(r'->\s*([A-Za-z_]\w*)\s*\(', text)}
    if 'new Redis(' in text:
        called.add('__construct')
    if re.search(r'->\s*(\$|\{)', text):
        called |= {word.lower() for word in re.findall(r"['\"]([A-Za-z_]\w*)['\"]", text)}
    for name in called & set(callers):
        callers[name].append(source.stem if source.suffix == '.php' else f'tests/{source.name}')

rows = []
counts = {'compared': 0, 'arguments only': 0, 'not compared': 0, 'not implemented': 0}
for name in methods:
    low = name.lower()
    scripts = [script for script in callers[low] if script != '175_signatures']
    if low in unimplemented:
        status, where = 'not implemented', 'Throws "Redis::%s is not implemented yet".' % low
    elif scripts:
        status = 'compared'
        shown = ', '.join(f'`{script}`' for script in scripts[:3])
        where = shown + (f' and {len(scripts) - 3} more' if len(scripts) > 3 else '')
    elif '175_signatures' in callers[low]:
        status, where = 'arguments only', 'Only its argument errors are compared.'
    else:
        status, where = 'not compared', ''
    counts[status] += 1
    note = not_run.get(low) or notes.get(low, '')
    rows.append(f'| `{name}` | {status} | {" ".join(part for part in [where, note] if part)} |')

table = (f'{counts["compared"]} compared, {counts["arguments only"]} with only their argument errors '
         f'compared, {counts["not compared"]} not compared, {counts["not implemented"]} not implemented.\n\n'
         '| Method | Status | Compared in, and notes |\n| --- | --- | --- |\n' + '\n'.join(rows) + '\n')
readme = root / 'README.md'
text = readme.read_text()
start, end = '<!-- inventory: tools/inventory.py -->\n', '<!-- /inventory -->'
before, rest = text.split(start)
_, after = rest.split(end)
readme.write_text(before + start + table + end + after)
print(counts)
