# Decisions

Every decision that shapes Phera is written down here: what was decided, why,
and what was turned down. The newest entries are at the top of each part. A
pull request that takes a decision adds its entry in the same pull request.

"Owner" means Gijs Rommers decided it. "Design" means it was decided during
design or review work and the owner can overrule it.

## Scope and compatibility

| Date | Decision | Why | Turned down | By |
| --- | --- | --- | --- | --- |
| 2026-11-19 | WordPress is a target next to Symfony and Laravel. `mysqli` and the `pg_*` functions are built as thin layers over the database drivers PDO already uses. | WordPress is the most used PHP application and talks to MySQL only through `mysqli`. One client per database keeps pooling, TLS and type mapping in one place. | A second, separate MySQL client for `mysqli`. | Owner |
| 2026-11-19 | Phera follows the newest PHP 8.5 patch release. A comparison whose PHP answer changed in a patch release is marked with the version it holds from and is skipped, visibly, on an older PHP. It is never removed. | Patch releases are bug fixes; CI moves to them on its own; nobody runs an old patch on purpose. | Pinning CI to one patch version. | Owner |
| 2026-11-19 | PHP 8.5 only. The two-version design for PHP 8.6 (architecture.md 6.3) is parked. | Decided by the owner after seeing the design. | Carrying 8.5 and 8.6 side by side now. | Owner |
| 2026-11-19 | `Fiber` stays as a complete class. It is a compatibility API (Monolog and others ask `Fiber::getCurrent()`), not part of Phera's concurrency, which is tasks on Tokio. | More real code runs unchanged. | Reducing it to `Fiber::getCurrent()` returning null. | Design |
| 2026-11-17 | A deprecated named item that nothing in use needs is not built and throws `Phera\NotSupportedException` or a compile error. Deprecated behaviour of a supported operation stays as in PHP, message included. | A clear error beats a silently different answer; the list is `crates/source/data/deprecations.tsv`. | Reproducing every legacy corner. | Owner |

## Safety and dependencies

| Date | Decision | Why | Turned down | By |
| --- | --- | --- | --- | --- |
| 2026-11-19 | TLS uses rustls with the `ring` provider, for databases, Redis and HTTP clients. A CI job scans the lock file for known advisories. | `ring` is audited and widely deployed. The pure Rust provider used before was a pre-release that calls itself not ready for production. `ring` holds C and assembly inside the dependency; Phera's own crates stay free of `unsafe`. | Keeping the pre-release provider; OpenSSL. | Design |
| 2026-11-19 | Cryptography (`openssl_*`, `sodium_*`, password hashing) is built from audited Rust crates. Phera implements no cipher, hash, padding or secret comparison itself. What no audited crate offers is left out and listed. | Home-made cryptography is where runtimes get broken. | Linking OpenSSL or libsodium. | Design |
| 2026-11-19 | DOM and XML are Phera's own safe tree with pure Rust parsers (`quick-xml`, `html5ever`), not libxml2. No external entity is ever loaded while parsing a string. `DOMDocument::loadHTML()` therefore differs from PHP for malformed HTML 4. | A C parser fed with documents from the network gives up the memory safety the runtime exists for. | Binding libxml2 for byte-exact output. | Design |
| 2026-11-19 | curl is built on `hyper` and rustls, not on libcurl. Only `http` and `https` exist; other protocols are refused with libcurl's error. | Same reason as DOM. Refusing `file://`, `gopher://` and the like also closes a class of request-forgery problems. | Binding libcurl. | Design |
| 2026-11-19 | `LOAD DATA LOCAL INFILE` is refused in PDO and `mysqli`. | It lets a database server read files of the client. | Supporting it behind an option. | Design |
| 2026-11-17 | No `unsafe` in Phera's own crates; a workspace lint forbids it and CI checks it. Where a design seems to need it, the answer is a safe design that may be slower. A specific use may be proposed to the owner for a gain of about ten times; it is never written without his decision. | The owner's rule. | | Owner |

## Server and requests

| Date | Decision | Why | Turned down | By |
| --- | --- | --- | --- | --- |
| 2026-11-19 | `phera serve` hides PHP error details in responses and logs them by default. `--dev` enables display. | Production logs retain diagnostics, including fatal traces. Local development can show the same errors in the response. | Displaying PHP errors in production by default. | Owner |
| 2026-11-19 | One deadline covers request headers and body. `--request-read-timeout` sets the initial budget, 60 seconds by default. Each 16 KiB received in the body adds a second. Expiry returns HTTP 408. | Credit for bytes received gives steady uploads time to finish. A trickle earns little credit, and the body-size limit bounds the extension. | A timeout that resets on each read, or a fixed deadline with no credit for upload progress. | Design |
| 2026-11-19 | Worker mode is supported too, through FrankenPHP's interface (`frankenphp_handle_request()`), so that Laravel Octane, Symfony's runtime packages and FrankenPHP applications run unchanged. Phera keeps many running copies of the worker script per thread, each answering one request at a time, so that waiting stays cheap. | The owner wants applications written for long-running use to run as they are. An existing interface means no new package for anyone to write. | A Phera-only worker interface; one copy of the worker script per thread, which would give up many requests per thread. This reverses the first draft of the design, which left worker mode out. | Owner |
| 2026-11-19 | Snapshot and fork is built in four steps: linked code shared per worker; a snapshot at `Phera\snapshot()` with a copy per request; adapters for Symfony and Laravel; copy on first write only if measurements ask for it (architecture.md 5.4). | One Laravel request costs 10 to 17 times PHP's time, and most of that is work repeated per request that opcache does once. Step 1 helps every application; the snapshot removes the framework's boot. | Offering only worker mode: state leaks between requests unless the application is written for it, so it cannot be the default for every application. | Owner asked for it; design |
| 2026-11-19 | When a web request ends, terminate an abandoned child and its process group while Phera still tracks the group leader. | A child can retain descriptors and consume resources after the response or deadline of a request. | PHP-FPM leaves ordinary children running. Waiting for them would tie request cleanup to the life of the child. | Design |
| 2026-11-19 | Program execution is enabled by default. `phera serve --no-process` makes every program launch entry point throw `Error`, including `shell_exec()` and backticks. | Frameworks expect process functions to be available. Deployments that never launch children can disable them together. | Disabling them by default or requiring a list of individual functions. | Design |
| 2026-11-19 | CLI `pcntl_fork()` throws `Phera\NotSupportedException`. | Forking leaves one thread in the child, which inherits runtime and allocator locks that other threads may have held. | Calling Unix `fork()` and continuing to run the interpreter in the child. | Design |
| 2026-11-19 | The execution time limit counts the CPU time a request spends running PHP code and is checked where the interpreter already hands over to the scheduler. Process and process-stream waits also count toward the budget. Other native calls check the limit when they return control to the interpreter. | A check per instruction would slow every script; waiting for a database must not count, as in PHP on Unix. | A wall-clock limit; a clock read in the interpreter loop. | Design |
| 2026-11-19 | A multipart boundary longer than 70 bytes is refused, and parsing stops when the request is abandoned. | RFC 2046's limit. A very long boundary made parsing take minutes after the client had gone (security round 3). | Accepting longer boundaries. | Design |

## Tools

| Date | Decision | Why | Turned down | By |
| --- | --- | --- | --- | --- |
| 2026-11-19 | The repository holds no directories of loose helper scripts. A helper worth keeping is a Rust test or a command of the tooling crate; a one-off stays outside the repository. Decisions go into this file, and the README is updated with every change a user can see. | The owner reads the repository to follow the project. | | Owner |
| 2026-11-19 | `phera lint` reports by default only what gave no false finding on the Symfony and Laravel code bases; unused variables, unused private properties, switch fall-through and self-comparison are among the defaults. Style, naming and complexity rules are opt-in. | A linter that cries wolf gets switched off. | All rules on by default. | Design |
| 2026-11-19 | `phera check` uses PHPStan's levels and identifiers, and reads a PHPStan baseline. | Projects can switch without relearning or rewriting their configuration. | Own level scheme. | Design |
| 2026-11-18 | Phera has no template engine of its own for now; Twig and Blade run as PHP. A native renderer is designed (architecture.md 11.6) and not built. | The frameworks' own engines must work anyway. | | Design |

## Speed and method

| Date | Decision | Why | Turned down | By |
| --- | --- | --- | --- | --- |
| 2026-11-19 | Speed is compared with PHP started as `php -n` (no Xdebug), with a second column for opcache and the JIT, and reported in cycles or CPU time next to instructions. | An earlier comparison used a PHP with Xdebug loaded and made Phera look faster than it is. Phera also retires fewer instructions per cycle than PHP, so instruction counts alone understate the gap. | Instruction counts as the only measure. | Design |
| 2026-11-19 | Built-in calls use the special forms PHP's own compiler has (`strlen`, `count`, type checks, `in_array` with a literal, `sprintf`, constant folding of pure functions). A shortcut tests properties of the call and never the spelling of a script. | They are observable in PHP only through speed, and each has a fixture proving equal behaviour, traces included. | Shortcuts tuned to benchmark scripts. | Design |
| 2026-11-18 | A change must not make any benchmark script more than 1 percent slower in instructions. The destructors work has a stated exception: no script more than 2 percent, at most three above 1 percent, the sum not slower, every overage named. | The interpreter loop is sensitive to layout; small costs add up unseen. Correct object lifetime has a real price. | No rule; a rule without exception. | Design |

## Process

| Date | Decision | Why | Turned down | By |
| --- | --- | --- | --- | --- |
| 2026-11-19 | CI runs the test suite as three jobs side by side, and a draft pull request gets only the three short checks. | One job took 20 to 24 minutes; work in progress filled the runners. | | Design |
| 2026-11-19 | Finished pull requests are merged into one batch branch, reviewed together and merged with one CI run. A batch merges only when a run of all php-src tests shows no test going from pass to fail. | Every merge makes the other branches out of date; the full php-src run finds regressions the repository's own selection does not. | Merging one by one. | Design |
| 2026-11-17 | Every pull request is reviewed against real PHP before it merges, whoever wrote it: its own scripts, the php-src tests, the benchmarks. The review is recorded on the pull request. | CI is only the first filter. | | Owner |
