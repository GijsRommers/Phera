# Decisions

Every decision that shapes Phera is written down here: what was decided, why,
and what was turned down. The newest entries are at the top of each part. A
pull request that takes a decision adds its entry in the same pull request.

"Owner" means Gijs Rommers decided it. "Design" means it was decided during
design or review work and the owner can overrule it.

## Scope and compatibility

| Date | Decision | Why | Turned down | By |
| --- | --- | --- | --- | --- |
| 2026-11-19 | Share SQL clients, TLS configuration, wire conversion, and the lease registry in `phera-db-drivers`. PDO retains its argument handling and SQLite statement API. | All three PHP APIs can reach the same database code independently. PDO preserves its existing configuration and reset behavior. | Adding database dependencies to the generic `phera-io-pool` crate or making mysqli and pgsql depend on PDO. | Design |
| 2026-11-19 | Keep mysqli report flags and the pgsql default connection in private VM request state. Scope their persistent lease keys to the worker. | Concurrent requests on a worker need independent PHP state. The worker-scoped keys reuse the SQL registry while preserving PDO's process-wide keys. | PHP-visible globals or thread-local PHP request state. | Design |
| 2026-11-19 | Reset a mysqli lease opened without a default database with COM_CHANGE_USER. Clear PostgreSQL notices after resetting a native lease. | This restores the original absence of a database and keeps cleanup notices out of the next request. | Reusing the session with the previous request's selected database or notices. | Design |
| 2026-11-19 | Retire a pooled MySQL connection after `change_user()`. | Its pool key records the original identity. | Lending the connection again under its old credentials. | Design |
| 2026-11-19 | Observe PostgreSQL column descriptions and command tags on the existing socket. Have PostgreSQL format prepared binary rows and launch async sends on the lease’s Tokio I/O runtime. | Keeps command status and session-dependent formats without executing statements twice. A busy PHP polling loop leaves I/O on its own runtime. Prepared rows currently cost an extra round trip per row. | Another PostgreSQL client or reconstructing session-dependent formats in Rust. | Design |
| 2026-11-19 | Require a server-side single-statement parse when a mysqli text query contains a statement separator. | The client negotiates multiple statements and exposes no API for toggling that capability after connecting. | Sending multi-statement text through the ordinary query entry point. | Design |
| 2026-11-19 | Reuse the PDO unbuffered cursor and allow native iterator steps to await input. Use it for mysqli single statements. | Each fetch or iterator advance can read one row while the worker serves other requests. The cursor owns the pending reply until it is consumed or drained, which preserves affected-row reporting and connection reuse. | Buffering at cursor creation or blocking the worker during iteration. | Design |
| 2026-11-19 | Use PHP 8.5's PostgreSQL stub as the alias list. Share parameter declarations and query handling with the canonical functions. | Retains the spelling and deprecation diagnostics applications see when they call an alias. | Registering names absent from the stub or giving aliases separate SQL implementations. | Design |
| 2026-11-19 | Use mysqlnd's decimal widening and year strings for mysqli and capture PostgreSQL prepared-result field sizes on the existing socket. | Wire types and PHP-visible types differ at these boundaries. PDO's conversion policy remains independent. | Changing the PDO defaults or inferring PostgreSQL field sizes from column names. | Design |
| 2026-11-19 | Record every mismatch in the database type and fetch matrices. | One failed test can expose many related differences while preserving original values and diagnostics. | Stopping at the first mismatch or dropping failing cases. | Design |
| 2026-11-19 | Keep all requested mysqli constants and their deprecations. Keep PgSql Lob type identity and defer large-object operations. | The requested surface includes deprecated constants. Large-object operations follow the core query and result work. | Suppressing diagnostics. | Design |
| 2026-11-19 | Send low-level COPY data through the shared PostgreSQL COPY sink. | Preserves the connection's transaction and authentication. | A separate COPY session or INSERT emulation. | Design |
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
| 2026-11-19 | Worker mode is supported too, through FrankenPHP's interface (`frankenphp_handle_request()`), so that Laravel Octane, Symfony's runtime packages and FrankenPHP applications run unchanged. Phera keeps many running copies of the worker script per thread, each answering one request at a time, so that waiting stays cheap. | The owner wants applications written for long-running use to run as they are. An existing interface means no new package for anyone to write. | A Phera-only worker interface; one copy of the worker script per thread, which would give up many requests per thread. This reverses the first draft of the design, which left worker mode out. | Owner |
| 2026-11-19 | Snapshot and fork is built in four steps: linked code shared per worker; a snapshot at `Phera\snapshot()` with a copy per request; adapters for Symfony and Laravel; copy on first write only if measurements ask for it (architecture.md 5.4). | One Laravel request costs 10 to 17 times PHP's time, and most of that is work repeated per request that opcache does once. Step 1 helps every application; the snapshot removes the framework's boot. | Offering only worker mode: state leaks between requests unless the application is written for it, so it cannot be the default for every application. | Owner asked for it; design |
| 2026-11-19 | A program started by a web request (`proc_open`, `exec`) is terminated when the request ends. `phera serve --no-process` forbids starting programs. `pcntl_fork` is not built. | One process serves many requests on few threads: an abandoned child would outlive its request unseen, and a multi-threaded async process cannot fork safely. | PHP-FPM's behaviour of letting children live. | Design |
| 2026-11-19 | The execution time limit counts the CPU time a request spends running PHP code and is checked where the interpreter already hands over to the scheduler. One long call into a built-in function is not interrupted. | A check per instruction would slow every script; waiting for a database must not count, as in PHP on Unix. | A wall-clock limit; a clock read in the interpreter loop. | Design |
| 2026-11-19 | A multipart boundary longer than 70 bytes is refused, and parsing stops when the request is abandoned. | RFC 2046's limit. A very long boundary made parsing take minutes after the client had gone (security round 3). | | Design |

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
