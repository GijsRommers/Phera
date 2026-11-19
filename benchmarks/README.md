# PHP runtime microbenchmarks

Run `cargo build --release -p phera-cli`, then `target/release/phera benchmarks/bench-objects.php` and `php -n benchmarks/bench-objects.php`. Repeat for each `bench-*.php`; stdout must match exactly.

Preserve the baseline binary before changing code. Record a starting reference series, then a final series, each with nine rounds in baseline/new/PHP order. Record whole-process CPU time (user + system, using `/usr/bin/time -l` or Python `resource.getrusage`) and monotonic wall time; report both medians and use CPU time for targets. Record `uptime` and other running work without changing processes belonging to others. If any final baseline median differs from its reference by more than 5%, repeat the final series, up to three series; report all unstable series and their load averages if none settles.

The additional cases exercise five million mixed method iterations, typed and array property mutation, interface-typed hierarchy calls, and construction plus loop-based merge sorting of 200,000 objects. They intentionally include compilation and process startup in timings.
Run `bench-exceptions.php` against Phera and `php -n` for one million calls, a throw every tenth call, and `finally`; its CPU target is at most 6 times PHP.
Run `bench-closures.php` for captures, arrow callbacks and method factories; target: 6x PHP CPU.

On macOS, run `python3 benchmarks/bench-http.py target/release/phera --baseline /path/to/main/phera`
to measure retired instructions for 200 and 1,000 PHP requests on one worker.
The script alternates baseline and candidate, keeps the HTTP connection open,
checks every response, and reports the median of six runs per binary. The
measurement includes starting and stopping the server and the first script
compilation. `--requests` and `--rounds` override the defaults.

The three `bench-function-*.php` scripts run their loops inside functions, which is where real code spends its time. The older scripts keep their loops at the top level of the file, and until November 2026 top-level code had its own, faster interpreter path, so those scripts alone gave too good a picture. Keep both kinds when comparing builds.

`bench-calls.php` mixes calls of many shapes (typed and untyped functions, defaults, variadics, instance, static, private and parent methods, getters returning floats, strings and arrays, constructors) inside one function. On 17 November 2026 it ran at 12 times PHP's CPU time; it is the reference for the call work in docs/architecture.md section 10.

## Measure inside the process

Until 17 November 2026 the comparisons with PHP used whole-process CPU time. `php -n` needs about 40 ms to start and Phera about 2 ms, so on scripts that run for 50 to 250 ms the start-up hid most of the difference: a loop reported as "1.4 times PHP" was really 5 times. Compare runtimes with a clock inside the script:

```php
$started = hrtime(true);
// the work
echo (hrtime(true) - $started) / 1e6, " ms\n";
```

Whole-process time is still right for comparing two builds of Phera with each other. Figures quoted as "times PHP" in commit messages before that date are too favourable to Phera by a factor of two to four.
