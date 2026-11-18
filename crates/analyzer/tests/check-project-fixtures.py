"""Check PHPT FILE sections independently, without ever executing their PHP code."""
import argparse
import concurrent.futures
import json
from pathlib import Path
import re
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--phera', type=Path, default=Path('target/release/phera'))
parser.add_argument('--phpstan', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--workers', type=int, default=8)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path('tests/fixtures')
phera, phpstan = str(args.phera.resolve()), str(args.phpstan.resolve())
config = args.output.resolve() / 'phpstan.neon'
config.write_text(f'parameters:\n    tmpDir: {json.dumps(str(args.output.resolve() / "cache"))}\n')
records = []

for source in sorted([*root.rglob('*.phpt'), *root.rglob('*.php')]):
    code = source.read_text(errors='replace')
    if source.suffix == '.phpt':
        section = re.search(r'^--FILE--\r?\n(.*?)(?=^--[A-Z_]+--|\Z)', code, re.M | re.S)
        if section is None:
            continue
        code = section.group(1)
    extracted = args.output / ('_'.join(source.relative_to(root).parts) + '.php')
    extracted.write_text(code)
    records.append({'file': str(source), 'source': str(extracted.resolve())})


def run(command):
    result = subprocess.run(command, capture_output=True, text=True, timeout=60)
    if result.returncode not in (0, 1):
        raise RuntimeError(f'{command}: {result.returncode}: {result.stderr}')
    return json.loads(result.stdout)


def messages(observation):
    return [message for file in observation.get('files', {}).values()
            for message in file['messages']]


def stan(record, level):
    return run([phpstan, 'analyse', '-c', str(config), '--level', str(level),
                '--no-progress', '--error-format=json', record['source']])


def check(record, tool):
    if tool == 'phera':
        record[tool] = run([phera, 'check', '--level', '0', '--no-progress',
                            '--format', 'json', record['source']])
    else:
        record[tool] = stan(record, 0)


timings = {}
for tool in ['phera', 'phpstan']:
    start = time.perf_counter()
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
        list(pool.map(lambda record: check(record, tool), records))
    timings[tool] = time.perf_counter() - start
    print(tool, len(records), timings[tool], 'seconds', flush=True)


def review(record):
    known = {(m['line'], m.get('identifier')) for m in messages(record['phpstan'])}
    missing = [d for d in record['phera']['diagnostics']
               if (d['line'], d['identifier']) not in known]
    if missing:
        record['phpstan_level1'] = stan(record, 1)
        known.update((m['line'], m.get('identifier'))
                     for m in messages(record['phpstan_level1']))
    record['unconfirmed'] = [d for d in missing if (d['line'], d['identifier']) not in known]


with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
    list(pool.map(review, records))
summary = {
    'files': len(records), 'workers': args.workers, 'phase_seconds': timings,
    'files_per_second': {tool: len(records) / seconds for tool, seconds in timings.items()},
    'phpstan_version': subprocess.check_output([phpstan, '--version'], text=True).strip(),
    'diagnostics': sum(len(r['phera']['diagnostics']) for r in records),
    'unconfirmed': sum(len(r['unconfirmed']) for r in records), 'results': records,
}
(args.output / 'summary.json').write_text(json.dumps(summary, indent=2))
print('Diagnostics:', summary['diagnostics'], 'unconfirmed:', summary['unconfirmed'])
