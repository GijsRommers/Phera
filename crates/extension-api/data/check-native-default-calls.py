"""Run stub-generated named and positional calls in fresh PHP/Phera processes.

Usage: python3 check-native-default-calls.py /path/to/phera [--php /path/to/php]
The fixture data supplies deterministic inputs and a receiver for instance methods.
"""

import argparse
import collections
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


HERE = Path(__file__).resolve().parent

INVENTORY = r'''
foreach (get_defined_functions()['internal'] as $name) {
    echo strtolower($name), "\t", $name, "\n";
}
foreach (array_merge(get_declared_classes(), get_declared_interfaces()) as $class) {
    $reflection = new ReflectionClass($class);
    foreach ($reflection->getMethods() as $method) {
        $key = strtolower($class . '::' . $method->getName());
        if ($method->isPublic()) echo $key, "\t", $class, "\t", $method->getName(), "\n";
    }
}
'''

PROBE = r'''<?php
date_default_timezone_set('UTC');
set_error_handler(static function (int $severity, string $message) {
    echo 'diagnostic ', $severity, ': ', $message, "\n";
    return true;
});
try {
    SETUP
    $result = CALL;
    CLEANUP
    if ($result instanceof DateTimeInterface) $result = $result->getTimezone()->getName();
    if (is_object($result)) $result = $result::class;
    if (is_resource($result)) $result = get_resource_type($result);
    echo 'value ', json_encode($result, JSON_INVALID_UTF8_SUBSTITUTE | JSON_PRESERVE_ZERO_FRACTION), "\n";
} catch (Throwable $error) {
    echo 'error ', $error::class, ': ', $error->getMessage(), "\n";
}
'''


# UNKNOWN supplies no positional value. Exercise an explicit final argument
# anyway, so an earlier skipped UNKNOWN is still tested against the oracle.
UNKNOWN_LAST = {
    'dateperiod::__construct': '0',
    'mt_rand': '10',
    'rand': '10',
    'stream_context_set_option': '"probe"',
}


def catalog():
    for line in (HERE / 'native-parameters.tsv').read_text().split('\n'):
        if not line or line.startswith('#'):
            continue
        name, location, *fields = line.split('\t')
        parameters = [
            tuple(fields[index:index + 4]) for index in range(0, len(fields), 4)
        ]
        yield name, location, parameters


def invoke(command, source, directory, private_key=None):
    environment = {
        'PATH': os.pathsep.join(['/opt/homebrew/bin', os.defpath]),
        'LC_ALL': 'C',
        'PHERA_NATIVE_DEFAULT_PROBE': 'present',
    }
    for name in ('PHERA_PHP_REAL', 'PHERA_PHP_TIMEZONEDB', 'PHERA_PHP_SHARED_EXTENSIONS'):
        if name in os.environ:
            environment[name] = os.environ[name]
    with tempfile.TemporaryDirectory(dir=directory) as temporary:
        work = Path(temporary).resolve()
        script = work / 'probe.php'
        script.write_text(source)
        if private_key is not None:
            (work / 'key.pem').write_text(private_key)
        run = subprocess.run(
            [*command, str(script)], cwd=work, env=environment, capture_output=True, timeout=10,
            check=False, text=True, errors='backslashreplace',
        )
        return (
            run.returncode,
            run.stdout.replace(str(work), '<probe>'),
            run.stderr.replace(str(work), '<probe>'),
        )


def inventory(phera, directory):
    status, output, diagnostics = invoke([phera], '<?php\n' + INVENTORY, directory)
    if status != 0 or diagnostics:
        raise RuntimeError(f'Could not read Phera registrations: {diagnostics}\n{output}')
    names = {}
    for line in output.splitlines():
        key, *spelling = line.split('\t')
        names[key] = spelling
    return names


def build_call(name, spelling, parameters, sample, named):
    arguments, receiver, setup, cleanup = sample
    assignments = [setup]
    supplied = []
    final = max(index for index, parameter in enumerate(parameters)
                if parameter[1].startswith('o-'))
    required = 0
    for index, (parameter, flags, default, _) in enumerate(parameters[:final + 1]):
        if flags[0] == 'r':
            value = arguments[required]
            required += 1
        elif named and index < final:
            continue
        else:
            value = default
        if '::' in value and 'random\\randomizer::' in name and value.startswith('IntervalBoundary'):
            value = 'Random\\' + value
        argument = f'$arg{index}'
        assignments.append(f'{argument} = {value};')
        supplied.append(f'{parameter}: {argument}' if named and index == final else argument)
    if required != len(arguments):
        raise ValueError(f'{name}: unexpected fixture argument count')
    if len(spelling) == 1:
        target, = spelling
    else:
        cls, method = spelling
        if method == '__construct':
            target = f'new {cls}'
        elif receiver:
            assignments.append(f'$receiver = {receiver};')
            target = f'$receiver->{method}'
        else:
            target = f'{cls}::{method}'
    call = f'{target}({", ".join(supplied)})'
    return PROBE.replace('SETUP', '\n    '.join(assignments)).replace('CALL', call).replace('CLEANUP', cleanup)


def openssl_key(php, directory):
    status, output, errors = invoke(php, r'''<?php
        $key = openssl_pkey_new(['private_key_bits' => 2048]);
        if (!$key || !openssl_pkey_export($key, $pem)) exit(1);
        echo $pem;
    ''', directory)
    if status != 0 or errors or not output.startswith('-----BEGIN PRIVATE KEY-----'):
        raise RuntimeError('The oracle could not create the temporary OpenSSL test key')
    return output


def main():
    options = argparse.ArgumentParser(description=__doc__)
    options.add_argument('phera', type=lambda path: str(Path(path).resolve()))
    homebrew = Path('/opt/homebrew/bin/php')
    options.add_argument('--php', default=os.getenv('PHERA_PHP') or
                         (str(homebrew) if homebrew.exists() else shutil.which('php')))
    options.add_argument('--report', type=Path)
    options.add_argument('--baseline', type=lambda path: str(Path(path).resolve()))
    arguments = options.parse_args()
    if not arguments.php:
        options.error('PHP 8.5 is required')
    php = [arguments.php, '-n', '-d', 'error_reporting=-1']
    samples = json.loads((HERE / 'native-default-call-samples.json').read_text())
    report = {'mismatches': {}, 'skipped': {}, 'counts': {}}
    counts = collections.Counter()
    with tempfile.TemporaryDirectory(prefix='phera-native-defaults-') as temporary:
        directory = Path(temporary)
        names = inventory(arguments.phera, directory)
        key = openssl_key(php, directory) if 'openssl_pkey_new' in names else None
        for name, location, parameters in catalog():
            if name not in names:
                continue
            optional = [p for p in parameters if p[1][0] == 'o' and p[1][1] != 'v']
            if len(optional) < 2:
                continue
            kind = 'methods' if '::' in name else 'functions'
            counts[f'eligible {kind}'] += 1
            sample = samples.get(name)
            if not isinstance(sample, list):
                report['skipped'][name] = sample or 'No deterministic isolated fixture'
                continue
            if optional[-1][2] == 'UNKNOWN':
                last = max(index for index, parameter in enumerate(parameters)
                           if parameter[1].startswith('o-'))
                parameter, flags, _, encoded = parameters[last]
                parameters[last] = (parameter, flags, UNKNOWN_LAST[name], encoded)
                counts[f'explicit last without default {kind}'] += 1
            named = build_call(name, names[name], parameters, sample, True)
            variants = {'named': named}
            if any(p[2] == 'UNKNOWN' for p in optional[:-1]):
                counts[f'unknown gap {kind}'] += 1
            else:
                variants['positional'] = build_call(name, names[name], parameters, sample, False)
            outputs = {}
            fixture = key if name.startswith('openssl_') else None
            for variant, source in variants.items():
                outputs[f'PHP {variant}'] = invoke(php, source, directory, fixture)
                outputs[f'Phera {variant}'] = invoke([arguments.phera], source, directory, fixture)
            counts[f'compared {kind}'] += 1
            if len(set(outputs.values())) != 1:
                mismatch = {'source': location, 'output': outputs}
                if arguments.baseline and 'positional' in variants:
                    before = invoke([arguments.baseline], variants['positional'], directory, fixture)
                    mismatch['baseline positional'] = before
                    if before == outputs['Phera positional'] == outputs['Phera named']:
                        counts[f'pre-existing positional difference {kind}'] += 1
                report['mismatches'][name] = mismatch
            else:
                counts[f'matched {kind}'] += 1
    report['counts'] = dict(counts)
    print(json.dumps(report['counts'], sort_keys=True))
    print(f"Skipped {len(report['skipped'])}, mismatched {len(report['mismatches'])}")
    if arguments.report:
        arguments.report.write_text(json.dumps(report, indent=2, sort_keys=True) + '\n')
    for name, result in report['mismatches'].items():
        print(f"{name} ({result['source']}): {result['output']}")
    for name, reason in report['skipped'].items():
        print(f'SKIP {name}: {reason}')
    return bool(report['mismatches'])


if __name__ == '__main__':
    raise SystemExit(main())
