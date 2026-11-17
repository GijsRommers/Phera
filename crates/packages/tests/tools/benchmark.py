"""Measure release Phera and Composer with isolated caches and identical fixture locks.

PHERA_PACKAGES_NETWORK=1 python3 benchmark.py target/release/phera composer.phar
Cold results are single runs; warm results are medians of three complete processes.
"""
import json
import os
from pathlib import Path
import resource
import shutil
import statistics
import subprocess
import sys
import time

TESTS = Path(__file__).resolve().parents[1]
REPO = TESTS.parents[2]
WORK = REPO / 'target/package-benchmarks'
WORK.mkdir(parents=True, exist_ok=True)
PHERA = str(Path(sys.argv[1]).resolve())
COMPOSER = str(Path(sys.argv[2]).resolve())
PHP = os.environ.get('PHERA_PHP', '/opt/homebrew/bin/php')
NAMES = ['laravel--laravel', 'league--commonmark', 'league--csv']


def measure(command, directory, environment, label):
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    started = time.perf_counter()
    result = subprocess.run(command, cwd=directory, env=environment, capture_output=True,
                            text=True, timeout=600)
    elapsed = time.perf_counter() - started
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    cpu = after.ru_utime + after.ru_stime - before.ru_utime - before.ru_stime
    (WORK / (label + '.log')).write_text(result.stdout + result.stderr)
    if result.returncode:
        raise RuntimeError(label + ' failed: ' + result.stderr[-500:])
    return {'wall': elapsed, 'cpu': cpu}


def median(samples):
    return {key: statistics.median(sample[key] for sample in samples) for key in ['wall', 'cpu']}


if os.environ.get('PHERA_PACKAGES_NETWORK') != '1':
    raise SystemExit('Set PHERA_PACKAGES_NETWORK=1 to opt into benchmarks.')
rows = []
for name in NAMES:
    fixture = TESTS / 'fixtures' / name
    provenance = json.loads((fixture / 'provenance.json').read_text())
    cache_root = WORK / (name + '-caches')
    if cache_root.exists():
        shutil.rmtree(cache_root)
    environment = dict(os.environ, COMPOSER_ROOT_VERSION=provenance['root_version'],
                       COMPOSER_HOME=str(cache_root / 'composer-home'),
                       COMPOSER_CACHE_DIR=str(cache_root / 'composer-cache'),
                       PHERA_CACHE_DIR=str(cache_root / 'phera-store'))
    directories = {}
    for tool in ['composer', 'phera']:
        destination = WORK / (name + '-' + tool)
        if destination.exists():
            shutil.rmtree(destination)
        source = REPO / 'target/package-comparison' / (name + '-composer')
        shutil.copytree(source, destination, ignore=shutil.ignore_patterns('vendor', '.git'))
        shutil.copyfile(fixture / 'composer.lock', destination / 'composer.lock')
        directories[tool] = destination
    for state in ['cold', 'warm', 'warm offline']:
        samples = {'composer': [], 'phera': []}
        for iteration in range(3 if state != 'cold' else 1):
            for tool in ['composer', 'phera']:
                directory = directories[tool]
                vendor = directory / 'vendor'
                if vendor.exists():
                    shutil.rmtree(vendor)
                command = [PHP, COMPOSER, 'install', '--no-plugins', '--no-interaction'] \
                    if tool == 'composer' else [PHERA, 'install', '--frozen-lockfile']
                command.extend(['--no-scripts', '--ignore-platform-reqs', '-o'])
                current_environment = dict(environment)
                if state == 'warm offline':
                    if tool == 'composer':
                        current_environment['COMPOSER_DISABLE_NETWORK'] = '1'
                    else:
                        command.append('--offline')
                samples[tool].append(measure(command, directory, current_environment,
                    name + '-' + state.replace(' ', '-') + '-' + tool + '-' + str(iteration)))
        row = {'fixture': name, 'operation': 'install', 'state': state,
               **{tool: median(values) for tool, values in samples.items()}}
        rows.append(row)
        print(json.dumps(row), flush=True)
    for tool in ['composer', 'phera']:
        metadata = cache_root / (
            'composer-cache/repo' if tool == 'composer' else 'phera-store/metadata')
        if metadata.exists():
            shutil.rmtree(metadata)
    for state in ['cold metadata', 'warm metadata']:
        samples = {'composer': [], 'phera': []}
        for iteration in range(3 if state == 'warm metadata' else 1):
            for tool in ['composer', 'phera']:
                directory = directories[tool]
                (directory / 'composer.lock').unlink(missing_ok=True)
                command = [PHP, COMPOSER, 'update', '--no-plugins',
                           '--no-interaction', '--no-audit'] \
                    if tool == 'composer' else [PHERA, 'update']
                command.extend(['--no-install', '--no-scripts', '--ignore-platform-reqs'])
                samples[tool].append(measure(command, directory, environment,
                    name + '-' + state.replace(' ', '-') + '-' + tool + '-' + str(iteration)))
        row = {'fixture': name, 'operation': 'update', 'state': state,
               **{tool: median(values) for tool, values in samples.items()}}
        rows.append(row)
        print(json.dumps(row), flush=True)
(WORK / 'results.json').write_text(json.dumps(rows, indent=2) + '\n')
