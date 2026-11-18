"""Regenerate PHPStan line sets and explain every difference, without executing fixtures."""
import argparse
import hashlib
import concurrent.futures
import json
import os
import re
from pathlib import Path
import subprocess
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--phpstan', default=os.environ.get('PHPSTAN', 'phpstan'))
parser.add_argument('--phera', default=os.environ.get('PHERA_BIN', 'target/debug/phera'))
parser.add_argument('--reuse-phpstan', action='store_true',
                    help='Reuse only cached PHPStan results with the same source hash and version')
parser.add_argument('--workers', type=int, default=min(os.cpu_count() or 1, 8))
parser.add_argument('--output', type=Path, default=Path('/private/tmp/phera-check-comparison'))
args = parser.parse_args()
corpus = Path('crates/analyzer/tests/corpus')
args.output.mkdir(parents=True, exist_ok=True)
phpstan_version = subprocess.check_output([args.phpstan, '--version'], text=True).strip()
reasons = (corpus / 'DIFFERENCES.md').read_text() if (corpus / 'DIFFERENCES.md').exists() else ''
explanations = set()
for line in reasons.splitlines():
    match = re.match(r'\| `([^`]+)` \| ([01]) \| (\[[^]]*\]) \| (\[[^]]*\]) \|', line)
    if match:
        name, level, stan, check = match.groups()
        explanations.add((name, int(level), tuple(json.loads(stan)), tuple(json.loads(check))))


def run(file):
    result = []
    phpstan_lines = []
    for level in (0, 1):
        cache = args.output / f'{file.stem}.level{level}.json'
        source_hash = hashlib.sha256(file.read_bytes()).hexdigest()
        previous = json.loads(cache.read_text()) if args.reuse_phpstan and cache.exists() else {}
        if (previous.get('source_sha256') == source_hash
                and previous.get('phpstan_version') == phpstan_version):
            stan_data = previous['phpstan']
        else:
            stan = subprocess.run([args.phpstan, 'analyse', '--no-progress', '--error-format=json',
                                   '--level', str(level), str(file)], capture_output=True, text=True)
            if stan.returncode not in (0, 1):
                raise RuntimeError(f'{file}: PHPStan failure\n{stan.stderr}')
            stan_data = json.loads(stan.stdout)
        check = subprocess.run([args.phera, 'check', '--no-progress', '--format', 'json',
                                '--level', str(level), str(file)], capture_output=True, text=True)
        if check.returncode not in (0, 1):
            raise RuntimeError(f'{file}: Phera failure\n{check.stderr}')
        check_data = json.loads(check.stdout)
        stan_messages = [m for data in stan_data.get('files', {}).values() for m in data['messages']]
        if stan_data.get('errors'):
            raise RuntimeError(f'{file}: PHPStan errors: {stan_data["errors"]}')
        stan_set = sorted({m['line'] for m in stan_messages})
        check_set = sorted({m['line'] for m in check_data['diagnostics']})
        phpstan_lines.append(f'{level}: ' + ','.join(map(str, stan_set)))
        output = {'phpstan': stan_data, 'phera': check_data,
                  'source_sha256': source_hash, 'phpstan_version': phpstan_version}
        (args.output / f'{file.stem}.level{level}.json').write_text(json.dumps(output, indent=2))
        if stan_set != check_set:
            result.append((level, stan_set, check_set))
    file.with_suffix('.phpstan').write_text('\n'.join(phpstan_lines) + '\n')
    return file.name, result


start = time.monotonic()
identical = explained = unexplained = 0
with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
    for name, differences in pool.map(run, sorted(corpus.glob('*.php'))):
        if not differences:
            identical += 1
            continue
        known = all((name, level, tuple(stan), tuple(check)) in explanations
                    for level, stan, check in differences)
        if known:
            explained += 1
        else:
            unexplained += 1
        for level, stan, check in differences:
            print(f'{name} level {level}: PHPStan {stan}; Phera {check}; '
                  + ('explained' if known else 'UNEXPLAINED'), flush=True)
print(f'{identical} identical files; {explained} explained differences; '
      f'{unexplained} unexplained differences; {time.monotonic() - start:.2f}s', flush=True)
raise SystemExit(bool(unexplained))
