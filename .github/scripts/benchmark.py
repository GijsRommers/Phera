#!/usr/bin/env python3
"""Compare identical head benchmark scripts with clocks inside each runtime."""

import argparse
import os
import pathlib
import re
import statistics
import subprocess
import tempfile


def measure(binary, wrapper):
    output = subprocess.run(
        [str(binary), str(wrapper)], check=True, capture_output=True, timeout=120
    ).stdout
    match = re.search(rb'\nPHERA_CI_BENCH_NS:([0-9]+)\n$', output)
    if match is None:
        raise RuntimeError(f'{binary}: missing in-process timing marker')
    return int(match[1]) / 1e6, output[:match.start()]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base', required=True, type=pathlib.Path)
    parser.add_argument('--head', required=True, type=pathlib.Path)
    parser.add_argument('--base-benchmarks', type=pathlib.Path)
    parser.add_argument('--runs', type=int, default=5)
    parser.add_argument('--base-sha', default='local base')
    parser.add_argument('--head-sha', default='local head')
    args = parser.parse_args()
    if args.runs < 3:
        parser.error('use at least three measured runs')
    binaries = {'base': args.base.resolve(), 'head': args.head.resolve()}
    report = [
        '## In-process benchmark comparison',
        '',
        f'Base: `{args.base_sha}`; head: `{args.head_sha}`.',
        f'Median of {args.runs} alternating runs after one warm-up per binary.',
        'Clock: `hrtime(true)` inside the script, around `require` of the same head script.',
        'This includes script loading/compilation, excludes process startup, and checks stdout parity.',
        '**Shared CI machines are noisy: differences under about 10% mean nothing here.**',
        '',
        '| Script | Base (ms) | Head (ms) | Change |',
        '| --- | ---: | ---: | ---: |',
    ]
    errors = []
    with tempfile.TemporaryDirectory(prefix='phera-bench-') as directory:
        wrapper = pathlib.Path(directory) / 'timed.php'
        for script in sorted(pathlib.Path('benchmarks').glob('bench-*.php')):
            # The wrapper leaves strict_types declarations and scripts untouched.
            path = str(script.resolve()).replace('\\', '\\\\').replace("'", "\\'")
            wrapper.write_text(
                '<?php\n$__phera_ci_started = hrtime(true);\n'
                f"require '{path}';\n"
                'echo "\\nPHERA_CI_BENCH_NS:", hrtime(true) - $__phera_ci_started, "\\n";\n'
            )
            try:
                if args.base_benchmarks is not None and not (args.base_benchmarks / script.name).is_file():
                    expected = measure(binaries['head'], wrapper)[1]
                    samples = []
                    for _ in range(args.runs):
                        elapsed, output = measure(binaries['head'], wrapper)
                        if output != expected:
                            raise RuntimeError('head stdout changed')
                        samples.append(elapsed)
                    report.append(f'| {script.name} | new script | {statistics.median(samples):.3f} | — |')
                    continue
                expected = measure(binaries['base'], wrapper)[1]
                if measure(binaries['head'], wrapper)[1] != expected:
                    raise RuntimeError('base/head stdout differs during warm-up')
                samples = {'base': [], 'head': []}
                for round_number in range(args.runs):
                    # Alternate order too, so neither binary always runs first.
                    order = ['base', 'head'] if round_number % 2 == 0 else ['head', 'base']
                    for label in order:
                        elapsed, output = measure(binaries[label], wrapper)
                        if output != expected:
                            raise RuntimeError(f'{label} stdout changed')
                        samples[label].append(elapsed)
                base, head = (statistics.median(samples[label]) for label in ['base', 'head'])
                change = (head / base - 1) * 100
                report.append(f'| {script.name} | {base:.3f} | {head:.3f} | {change:+.1f}% |')
            except (RuntimeError, subprocess.SubprocessError) as error:
                # Keep the remaining observations visible even when one script fails.
                report.append(f'| {script.name} | error | error | unavailable |')
                errors.append(f'{script.name}: {error}')
    if errors:
        report.extend(['', '### Failed observations', '', *errors])
    markdown = '\n'.join(report) + '\n'
    print(markdown)
    if summary := os.environ.get('GITHUB_STEP_SUMMARY'):
        with open(summary, 'a') as file:
            file.write(markdown)
    if errors:
        raise SystemExit(1)


if __name__ == '__main__':
    main()
