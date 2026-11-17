# PHP runtime microbenchmarks

Run `cargo build --release -p phera-cli`, then `target/release/phera benchmarks/bench-objects.php` and `php -n benchmarks/bench-objects.php`. Repeat for each `bench-*.php`; stdout must match exactly.

Preserve the baseline binary before changing code. Record a starting reference series, then a final series, each with nine rounds in baseline/new/PHP order. Record whole-process CPU time (user + system, using `/usr/bin/time -l` or Python `resource.getrusage`) and monotonic wall time; report both medians and use CPU time for targets. Record `uptime` and other running work without changing processes belonging to others. If any final baseline median differs from its reference by more than 5%, repeat the final series, up to three series; report all unstable series and their load averages if none settles.

The additional cases exercise five million mixed method iterations, typed and array property mutation, interface-typed hierarchy calls, and construction plus loop-based merge sorting of 200,000 objects. They intentionally include compilation and process startup in timings.
Run `bench-exceptions.php` against Phera and `php -n` for one million calls, a throw every tenth call, and `finally`; its CPU target is at most 6 times PHP.
Run `bench-closures.php` for captures, arrow callbacks and method factories; target: 6x PHP CPU.

The three `bench-function-*.php` scripts run their loops inside functions, which is where real code spends its time. The older scripts keep their loops at the top level of the file, and until November 2026 top-level code had its own, faster interpreter path, so those scripts alone gave too good a picture. Keep both kinds when comparing builds.
