"""Compare complete production file sets, with fresh PHPStan caches on every measured run."""
import argparse
import json
from pathlib import Path
import statistics
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--code-root', type=Path, required=True)
parser.add_argument('--phera', type=Path, default=Path('target/release/phera'))
parser.add_argument('--phpstan', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--runs', type=int, default=3)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
phera, phpstan = str(args.phera.resolve()), str(args.phpstan.resolve())


def files_under(path, skip_vendor=True):
    return sorted(p.resolve() for p in path.rglob('*.php')
                  if not {'Tests', 'tests', '.github'}.intersection(p.relative_to(path).parts)
                  and (not skip_vendor or 'vendor' not in p.relative_to(path).parts))


reports = []
for name, roots in [('options-resolver', ['.', 'vendor/symfony']),
                    ('container', ['src', 'vendor/psr']), ('password_compat', ['lib'])]:
    root = args.code_root / name
    files = sorted({str(p) for part in roots for p in files_under(root / part)})
    (args.output / f'{name}.files.json').write_text(json.dumps(files, indent=2))
    timings = {'phera': [], 'phpstan': []}
    diagnostics = {}
    for run in range(args.runs):
        with tempfile.TemporaryDirectory(prefix='phpstan-', dir=args.output) as cache:
            config = Path(cache) / 'phpstan.neon'
            config.write_text(f'parameters:\n    tmpDir: {json.dumps(cache + "/cache")}\n')
            commands = {
                'phera': [phera, 'check', '--level', '0', '--no-progress', '--format', 'json', *files],
                'phpstan': [phpstan, 'analyse', '-c', str(config), '--level', '0',
                            '--no-progress', '--error-format=json', *files],
            }
            order = ['phera', 'phpstan'] if run % 2 == 0 else ['phpstan', 'phera']
            for tool in order:
                command = commands[tool]
                start = time.perf_counter()
                result = subprocess.run(command, capture_output=True, text=True)
                timings[tool].append(time.perf_counter() - start)
                if result.returncode not in (0, 1):
                    raise RuntimeError(f'{name}: {tool}: {result.stderr}')
                diagnostics[tool] = json.loads(result.stdout)
    report = {'name': name, 'commit': subprocess.check_output(
        ['git', '-C', str(root), 'rev-parse', 'HEAD'], text=True).strip(), 'files': len(files),
        'runs': timings, 'seconds': {k: statistics.median(v) for k, v in timings.items()}}
    report['files_per_second'] = {k: len(files) / v for k, v in report['seconds'].items()}
    report['diagnostics'] = diagnostics
    reports.append(report)
    (args.output / f'{name}.json').write_text(json.dumps(report, indent=2))
    print(name, report['files'], report['seconds'],
          'Phera diagnostics', len(diagnostics['phera']['diagnostics']), flush=True)
(args.output / 'summary.json').write_text(json.dumps(reports, indent=2))
