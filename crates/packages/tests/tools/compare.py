"""Explicit network Composer differential suite.

PHERA_PACKAGES_NETWORK=1 python3 compare.py /path/to/phera /path/to/composer.phar
Results, source archives and logs are written below this checkout's target/ directory.
"""
import concurrent.futures
import io
import json
import os
from pathlib import Path, PurePosixPath
import resource
import shutil
import subprocess
import sys
import time
import tarfile
import urllib.request
import zipfile

TESTS = Path(__file__).resolve().parents[1]
REPO = TESTS.parents[2]
WORK = REPO / 'target' / 'package-comparison'
WORK.mkdir(parents=True, exist_ok=True)
PHERA = str(Path(sys.argv[1]).resolve())
COMPOSER = str(Path(sys.argv[2]).resolve())
PHP = os.environ.get('PHERA_PHP', '/opt/homebrew/bin/php')
ENV = dict(os.environ, COMPOSER_CACHE_DIR=str(WORK / 'composer-cache'),
           COMPOSER_HOME=str(WORK / 'composer-home'), PHERA_CACHE_DIR=str(WORK / 'phera-store'))


def run(command, directory, env, name):
    start = time.perf_counter()
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    result = subprocess.run(command, cwd=directory, env=env, capture_output=True,
                            text=True, timeout=600)
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    elapsed = time.perf_counter() - start
    cpu = after.ru_utime + after.ru_stime - before.ru_utime - before.ru_stime
    (WORK / (directory.name + '-' + name + '.log')).write_text(result.stdout + result.stderr)
    return result, {'wall': elapsed, 'cpu': cpu}


def source(fixture, destination):
    provenance = json.loads((fixture / 'provenance.json').read_text())
    cache = WORK / (fixture.name + '.zip')
    if not cache.exists():
        request = urllib.request.Request(provenance['dist'], headers={'User-Agent': 'Phera-tests'})
        with urllib.request.urlopen(request, timeout=120) as reply:
            cache.write_bytes(reply.read())
    archive = zipfile.ZipFile(cache)
    for entry in archive.infolist():
        path = PurePosixPath(entry.filename)
        if path.is_absolute() or '..' in path.parts or '\\' in entry.filename:
            raise RuntimeError('unsafe source archive')
        parts = path.parts[1:]
        if not parts or parts[0] in ['vendor', '.git']:
            continue
        target = destination.joinpath(*parts)
        if entry.is_dir():
            target.mkdir(parents=True, exist_ok=True)
        else:
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(archive.read(entry))
            target.chmod(0o644 | ((entry.external_attr >> 16) & 0o111))
    manifest = json.loads((fixture / 'composer.json').read_text())
    required = []
    for key in ['autoload', 'autoload-dev']:
        required.extend(manifest.get(key, {}).get('files', []))
    lock = json.loads((fixture / 'composer.lock').read_text())
    required.extend(package['dist']['url'] for key in ['packages', 'packages-dev']
                    for package in lock[key] if package.get('dist', {}).get('type') == 'path')
    if any(not (destination / path).exists() for path in required):
        # Release archives may export-ignore files referenced by root dev autoloading,
        # or directories used by local repositories. Read the exact git tree instead.
        parts = provenance['dist'].split('/')
        if 'repos' not in parts:
            raise RuntimeError('root source context needs a GitHub source reference')
        index = parts.index('repos')
        repo = '/'.join(parts[index + 1:index + 3])
        bare = WORK / 'source-objects' / fixture.name
        bare.parent.mkdir(parents=True, exist_ok=True)
        if not bare.exists():
            subprocess.run(['git', 'init', '--bare', '--quiet', str(bare)], check=True)
            subprocess.run(['git', '-C', str(bare), 'fetch', '--quiet', '--depth=1',
                            'https://github.com/' + repo + '.git', provenance['reference']],
                           check=True, timeout=180)
        (bare / 'info/attributes').write_text('* -export-ignore\n')
        raw = subprocess.run(['git', '-C', str(bare), 'archive', '--format=tar', 'FETCH_HEAD'],
                             check=True, capture_output=True).stdout
        with tarfile.open(fileobj=io.BytesIO(raw)) as archive:
            for entry in archive:
                path = PurePosixPath(entry.name)
                if path.is_absolute() or '..' in path.parts or entry.issym() or entry.islnk():
                    raise RuntimeError('unsafe source tree entry')
                target = destination.joinpath(*path.parts)
                if entry.isdir():
                    target.mkdir(parents=True, exist_ok=True)
                elif entry.isfile():
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_bytes(archive.extractfile(entry).read())
                    target.chmod(0o644 | (entry.mode & 0o111))
    for name in ['composer.json', 'composer.lock']:
        shutil.copyfile(fixture / name, destination / name)
    return provenance


def tree(directory):
    result = {}
    for path in directory.rglob('*'):
        name = str(path.relative_to(directory))
        if path.is_symlink():
            result[name] = ('link', os.readlink(path))
        elif path.is_file():
            result[name] = (path.read_bytes(), path.stat().st_mode & 0o111)
        elif path.is_dir():
            result[name] = ('directory', path.stat().st_mode & 0o111)
    return result


def compare(fixture):
    left, right = WORK / (fixture.name + '-composer'), WORK / (fixture.name + '-phera')
    for directory in [left, right]:
        if directory.exists():
            shutil.rmtree(directory)
        directory.mkdir()
    provenance = source(fixture, left)
    shutil.copytree(left, right, dirs_exist_ok=True)
    env = dict(ENV, COMPOSER_ROOT_VERSION=provenance['root_version'])
    flags = ['--no-scripts', '--ignore-platform-reqs', '-o']
    oracle, composer_time = run([PHP, COMPOSER, 'install', '--no-plugins', '--no-interaction']
                               + flags, left, env, 'install')
    actual, phera_time = run([PHERA, 'install', '--frozen-lockfile'] + flags,
                            right, env, 'install')
    row = {'fixture': fixture.name, 'composer_install': oracle.returncode,
           'phera_install': actual.returncode, 'composer_time': composer_time,
           'phera_time': phera_time}
    vendor = json.loads((fixture / 'composer.json').read_text()).get('config', {}).get('vendor-dir', 'vendor')
    if not oracle.returncode and not actual.returncode:
        a, b = tree(left / vendor), tree(right / vendor)
        different = [path for path in sorted(a.keys() | b.keys()) if a.get(path) != b.get(path)]
        generated_files = {'autoload.php', 'composer/LICENSE', 'composer/installed.json',
            'composer/installed.php', 'composer/platform_check.php'}
        generated_files.update('composer/autoload_' + name + '.php' for name in
            ['psr4', 'namespaces', 'classmap', 'files', 'static', 'real'])
        generated = lambda path: path in generated_files or path.startswith('bin/')
        row['package_differences'] = [path for path in different if not generated(path)]
        row['generated_differences'] = [path for path in different if generated(path)]
        if row['package_differences']:
            row['package_difference_types'] = {path: [repr(a.get(path))[:200],
                repr(b.get(path))[:200]] for path in row['package_differences']}
        a = json.loads((left / vendor / 'composer/installed.json').read_text())
        b = json.loads((right / vendor / 'composer/installed.json').read_text())
        row['installed_identical'] = a == b
        if a != b:
            row['installed_difference_packages'] = [x['name'] for x, y in zip(a['packages'], b['packages']) if x != y]
        probes = []
        for directory in [left, right]:
            result, _ = run([PHP, '-n', '-d', 'display_errors=stderr', '-d', 'log_errors=0', str(TESTS / 'tools/probe.php'), vendor], directory, env, 'probe')
            output = result.stdout.replace(str(directory), '.')
            diagnostics = result.stderr.replace(str(directory), '.')
            if not result.returncode:
                prefix, payload = output.rsplit('\nPHERA_PROBE_JSON=', 1)
                probes.append({'result': json.loads(payload), 'output': prefix, 'diagnostics': diagnostics})
            else:
                probes.append({'status': result.returncode, 'output': output, 'diagnostics': diagnostics})
        row['autoload_identical'] = probes[0] == probes[1]
        if probes[0] != probes[1]:
            (WORK / (fixture.name + '-autoload.json')).write_text(json.dumps(probes, indent=2))
    for directory in [left, right]:
        (directory / 'composer.lock').unlink(missing_ok=True)
    oracle, composer_update = run([PHP, COMPOSER, 'update', '--no-install', '--no-plugins',
        '--no-scripts', '--ignore-platform-reqs', '--no-interaction', '--no-audit'], left, env, 'update')
    actual, phera_update = run([PHERA, 'update', '--no-install', '--no-scripts',
        '--ignore-platform-reqs'], right, env, 'update')
    row.update(composer_update=oracle.returncode, phera_update=actual.returncode,
               composer_update_time=composer_update, phera_update_time=phera_update)
    if not oracle.returncode and not actual.returncode:
        versions = lambda path: {p['name']:p['version'] for key in ['packages','packages-dev']
                                for p in json.loads(path.read_text())[key]}
        a, b = versions(left / 'composer.lock'), versions(right / 'composer.lock')
        row['resolution_differences'] = {key: [a.get(key), b.get(key)] for key in sorted(a.keys() | b.keys()) if a.get(key) != b.get(key)}
        validation, _ = run([PHP, COMPOSER, 'validate', '--no-check-publish'], right, env, 'validate')
        row['composer_accepts_lock'] = 'not up to date' not in validation.stdout + validation.stderr
        if (right / vendor).exists():
            shutil.rmtree(right / vendor)
        handoff, _ = run([PHP, COMPOSER, 'install', '--no-plugins', '--no-interaction',
                          '--no-scripts', '--ignore-platform-reqs', '-o'], right, env, 'handoff')
        row['composer_installs_phera_lock'] = handoff.returncode == 0
    (WORK / (fixture.name + '-result.json')).write_text(json.dumps(row, indent=2))
    print(json.dumps(row), flush=True)
    return row


if os.environ.get('PHERA_PACKAGES_NETWORK') != '1':
    raise SystemExit('Set PHERA_PACKAGES_NETWORK=1 to opt into registry comparisons.')
fixtures = sorted(path for path in (TESTS / 'fixtures').iterdir() if (path / 'composer.lock').exists())
if len(sys.argv) > 3:
    fixtures = [path for path in fixtures if path.name in sys.argv[3:]]
rows = []
# Timings are sequential so process CPU accounting does not combine concurrent children.
for fixture in fixtures:
    try:
        rows.append(compare(fixture))
    except Exception as error:
        row = {'fixture': fixture.name, 'error': str(error)}
        rows.append(row)
        print(json.dumps(row), flush=True)
(WORK / 'results.json').write_text(json.dumps(rows, indent=2) + '\n')
commands = ['composer_install', 'phera_install', 'composer_update', 'phera_update']
required = ['installed_identical', 'autoload_identical', 'composer_accepts_lock',
            'composer_installs_phera_lock']
if any(row.get('error') or any(row.get(key, 1) != 0 for key in commands)
       or row.get('package_differences') or row.get('resolution_differences')
       or not all(row.get(key, False) for key in required) for row in rows):
    raise SystemExit(1)
