# Phera architecture

Phera runs modern PHP applications (Symfony, Laravel) in one process that serves
many requests at once. This document is the plan: what gets built, how the pieces
fit, and what is left out on purpose.

## 1. Pipeline

```
PHP source ─▶ lexer ─▶ parser ─▶ AST ─▶ compiler ─▶ bytecode ─▶ VM ─▶ runtime ─▶ server
```

| Crate | Role | Depends on |
| --- | --- | --- |
| `phera-source` | Spans and source files | |
| `phera-lexer` | Source to tokens, matching `token_get_all` | source |
| `phera-ast` | Syntax tree definitions | source |
| `phera-parser` | Tokens to AST | lexer, ast |
| `phera-value` | PHP values and their semantics: strings, arrays, conversions, comparison, arithmetic | |
| `phera-bytecode` | Instruction set and compiled units. Plain data, `Send + Sync` | |
| `phera-compiler` | AST to bytecode, name resolution | ast, parser, bytecode |
| `phera-vm` | Executes bytecode: frames, classes, exceptions, generators | bytecode, value |
| `phera-extension-api` | What native functions and classes see | value |
| `phera-extensions/*` | Native functions grouped like PHP extensions | extension-api |
| `phera-runtime` | Code cache, includes, autoload, superglobals, request lifecycle | compiler, vm, extensions |
| `phera-server` | HTTP on Tokio, one worker thread per core | |
| `phera-cli` | `phera run file.php`, `phera serve` | runtime, server |

**Safe Rust only.** The workspace sets `unsafe_code = "forbid"`, so no Phera
crate can contain `unsafe`. Where a design seems to need it (the value
representation, the array, the interpreter loop), the answer is a safe design
that is a little slower, not an exception to the rule.

**No stack overflows.** A stack overflow cannot be caught. It aborts the process,
and with it every request on every worker. So nothing may recurse in proportion
to its input without a limit:

- The parser runs on its own 256 MB stack and refuses nesting deeper than 1000
  levels with an ordinary error.
- Long chains (`a . b . c ...`, `$a->b()->c()...`) are built, walked and dropped
  in loops, never by recursion. Generated code has chains thousands of terms long.
- The compiler may recurse on nesting, which the parser has bounded, but must
  loop over chains.
- The VM never uses the Rust stack for PHP calls, and values drop iteratively,
  so a million-element linked list or a deeply nested array is safe to free.

Dependencies point one way. The VM never sees the AST, and the server never sees
the VM: it talks to a `Handler` trait that the runtime implements.

## 2. Concurrency: M:N on Tokio, thread per core

The model is the one Java calls virtual threads. A request is a cheap task, many
tasks share a few OS threads, and a task that waits for I/O gives its thread away.

- **Workers.** `phera serve` starts one OS thread per core. Each thread runs a
  single-threaded Tokio runtime with a `LocalSet` and its own listening socket
  bound with `SO_REUSEPORT`, so the kernel spreads connections over workers.
- **Tasks.** Each request is `spawn_local`ed on the worker that accepted it and
  stays there. A task owns one `Vm`: its frames, its globals, its objects.
- **No `Send` values.** Because a task never moves between threads, PHP values use
  `Rc` and `RefCell`, not `Arc` and locks. This is the main reason for thread per
  core: reference counting is the hottest operation in a PHP runtime.
- **Suspension.** The interpreter loop is an `async fn`. PHP frames live on the
  heap in a `Vec<Frame>`, never on the Rust stack, so a PHP call is a push and a
  suspended request is just a `Vm` that is not being polled.
- **Native functions come in two kinds.** Sync ones (`strlen`) are plain function
  pointers. Async ones (`PDO::query`, `file_get_contents`, `sleep`, `curl_exec`)
  return a future that the interpreter awaits. A native function must never block
  the thread: blocking one worker stalls every request on it. Work with no async
  API (bcrypt, file system calls) goes through `tokio::task::spawn_blocking`.
- **Fairness.** The interpreter counts backward jumps and calls, and yields to
  Tokio every few thousand, so a CPU-bound request cannot starve its neighbours.
- **Shared, immutable code.** Compiled bytecode is `Arc`-shared across all
  workers through the code cache. Everything mutable is per task.

## 3. Values

Defined in `phera-value`.

```rust
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(PhpString),            // byte string, cheap to clone, copy-on-write
    Array(Rc<PhpArray>),          // ordered hash map, copy-on-write via Rc::make_mut
    Object(Rc<Object>),           // handle semantics: clones share the object
    Resource(Rc<Resource>),       // streams and other handles
    Reference(Rc<RefCell<Value>>),// a PHP `&` reference; only found in variable and array slots
}
```

- **Arrays are values, objects are handles.** Assigning an array copies it in
  PHP. `Rc<PhpArray>` makes that copy free until someone writes.
- **`PhpArray`** keeps insertion order, uses `int` and string keys with PHP's key
  normalisation (`"5"` becomes `5`), tracks the next free integer key, and has
  an internal cursor for `current()` and `next()`.
- **Objects** hold a class handle, a property table, and an optional slot of
  native state for built-in classes such as `Closure`, `ArrayObject` or `PDO`.
- **Memory.** Reference counting frees most things at once, which also gives
  PHP's destructor timing. Cycles are handled per request: the `Vm` keeps a weak
  list of the objects it created and clears their properties when the request
  ends. A cycle collector inside long requests can come later.

## 4. Bytecode and VM

- **Register based.** Each function has a fixed number of slots. Named local
  variables get the first slots, resolved at compile time; temporaries follow.
  `$a = $b + 1` is one instruction, `Add { dst, lhs, rhs }`, with no stack traffic.
- **Instructions are a Rust `enum`,** not hand-packed bytes. It is simpler,
  safer, and the `match` compiles to a jump table.
- **A compiled file** holds its functions, class declarations, constants and a
  top-level function. It contains no `Rc`, so it is `Send + Sync`.
- **Dynamic scope is the exception.** A function that uses `$$name`, `extract`,
  `compact`, `get_defined_vars`, `include` or `eval` is compiled with a name
  table so its locals can be reached by name. All other functions skip that cost.
- **Exceptions** use a handler table per function (`try` range, `catch` and
  `finally` targets). Throwing unwinds the heap frames; no Rust panics.
- **Generators** fall out of heap frames: a generator is a frame detached from
  the stack and resumed later.
- **A call is several instructions,** as in PHP itself: find the callee, pass
  each argument in source order, then call. Only the callee knows which
  parameters are by reference, so this gives PHP's evaluation order and the
  right order of warnings.
- **Calls into PHP from native code** (`array_map`, `usort`, autoloaders) are
  async native functions that ask the VM to run a callable and await it.

## 5. Request isolation

Phase 1 is the classic PHP model made cheap: each request gets a fresh `Vm` and
runs the front controller from the top. Parsing and compiling are paid once, in
the shared code cache. This is correct for every application with no changes.

Under `phera run`, `php_sapi_name()` and `PHP_SAPI` both report `cli`.
Under `phera serve`, they report `phera`, while PHP's development server reports `cli-server`.

`getallheaders()` and `apache_request_headers()` currently use the lower-case
names in Hyper's header map. Preserving client spelling would require
`crates/server` to capture the original names at HTTP/1 parsing and pass them
alongside the normalized headers to the runtime.

Phase 2 is snapshot and fork. A worker boots the application once (autoloader,
container, routes), freezes that heap as a snapshot, and gives each request a
fork of it. Arrays and strings are already copy-on-write, so a fork copies only
objects and references, lazily where possible. Rules that make this safe:

- A snapshot must hold no live resources. Database connections and open files
  are created per request, or come from a pool the runtime owns.
- The split between boot and handle must be explicit. Phera follows the same
  contract as the Symfony Runtime component and Laravel Octane.
- Static properties and globals are part of the fork, so no request can see
  another's changes.

### 5.1 Limits on a request

Many requests share a worker thread, and many workers share a process, so a
request that runs away takes others with it. Hostile-client testing showed
what is missing (docs/security/http-findings.md, F3, F4, F5, F7). The limits,
each with a default that a `[server]` section in `phera.toml` or a flag can
change:

| Limit | Default | Where it is enforced | Built |
| --- | --- | --- | --- |
| Whole request, waiting included (`--request-timeout`) | 60 s | a timer around the request's task. The task is stopped at its next wait or hand-over point and the client gets 504 | yes |
| A silent connection: before its first request, between two requests, and while a request head arrives | 75 s | hyper's header read timer, which covers all three. Longer than the 60 s after which proxies commonly drop idle connections, so that Phera is never the side that closes a connection the proxy is about to reuse | yes |
| Pause between two pieces of a request body | 30 s | a timer on each read; 408 and the connection is closed | yes |
| Size of a request body (`post_max_size`) | 8 MiB | while reading; 413 beyond it | yes |
| Open connections per worker | 10,000 | a worker takes a free slot before it accepts; beyond the limit, connections wait in the operating system's queue | yes |
| Running time of a script (`max_execution_time`, `set_time_limit()`) | 30 s under `serve`, none on the command line | at the interpreter's hand-over points (every 65,536 loop rounds or calls), which already exist; the check is one clock read there, and the request ends with PHP's "Maximum execution time of N seconds exceeded" | no |
| Memory of one request (`memory_limit`) | 128 MiB | to be decided | no |
| Buffered response | 64 KiB, then it streams | output is handed to the connection in chunks once the buffer passes the threshold, and the script waits while the client is slow (so a slow reader costs a suspended task, not memory) | no; static files are sent in pieces already |

The request timeout and PHP's own `max_execution_time` are two limits, as
with nginx and PHP-FPM: the first counts everything, waiting for a database
included, and ends in a plain 504; the second counts running time and ends
in PHP's fatal error, which a shutdown function can still see.

Errors: under `serve`, an uncaught error answers 500 with a generic body and
writes the message, file and line to the log; `display_errors=1` shows them in
the response, as PHP does, and is the default only on the command line.

### 5.2 `phera serve` is the production server

The goal is one binary that takes the place of PHP-FPM and the web server
in front of it, the way FrankenPHP does: it accepts the requests, serves the
static files, and runs the PHP. `php -S` is only the yardstick for what a
script sees in `$_SERVER`, `$_POST` and the response; it is not the model
for the server.

**Where it runs.** In practice behind a reverse proxy: a hosting platform's
edge, a load balancer, an ingress controller. That proxy owns the
certificates, speaks HTTP/2 and HTTP/3 to browsers, and sends plain
HTTP/1.1 to a container. So the work that matters most is being a good
upstream, not being an edge server.

The list below is the order of work. Built so far (2026-11-18): steps 1,
2, 3 and 8, and of step 5 the platform's `PORT`, the health path and the
graceful end on `SIGTERM`. The server speaks HTTP/1.1 with keep-alive and
runs one worker per core.

1. **Limits and timeouts** (5.1). A proxy does not protect a worker from a
   script that never ends or a body that is too large.
2. **Static files, properly.** Sent from disk in chunks, never held whole in
   memory; `ETag` and `Last-Modified` with 304 answers; `Range` (206) for
   video and resumed downloads; `HEAD`; a precompressed sibling
   (`app.js.br`, `.zst`, `.gz`) when the client accepts it; a
   `Cache-Control` rule per path. PHP source, dot files and anything outside
   the document root are never served.
3. **Knowing the proxy.** A list of trusted proxies decides when
   `X-Forwarded-For`, `X-Forwarded-Proto`, `X-Forwarded-Host` and
   `Forwarded` set `REMOTE_ADDR`, `HTTPS`, `SERVER_PORT` and `HTTP_HOST`.
   Without the list those headers are ignored, since any client can send
   them. (`--trusted-proxies 10.0.0.0/8,...`, or the word `private` for
   every private range.)
4. **HTTP/2 on the same port**, without TLS ("h2c"), for proxies that speak
   HTTP/2 to their upstreams: many requests share one connection. The
   server answers HTTP/1.1 and HTTP/2 on one listener; a script sees no
   difference except `SERVER_PROTOCOL`.
5. **Being a good container.** Listening on the address and port the
   platform gives (`PORT`), a health endpoint that tells "started" from
   "ready", finishing in-flight requests on `SIGTERM` within the platform's
   grace period, logs as lines on standard output, and no need to write
   anywhere but a temporary directory.
6. **Boot once.** Snapshot and fork (section 5, phase 2) is Phera's answer
   to FrankenPHP's worker mode and Laravel Octane: the framework boots once
   per worker and every request starts from that state. Because a request
   works on a fork, it cannot leak state into the next one, which is the
   known risk of those worker modes.
7. **Finishing early.** `fastcgi_finish_request()` (send the response, keep
   working), which Symfony calls when it exists, and Early Hints (103),
   which the proxy passes on.
8. **Compression** of PHP's responses (gzip, zstd, brotli) where the proxy
   does not compress.
9. **Operations.** A `phera.toml`, an access log, metrics, reloading code
   and configuration without dropping connections.
10. **HTTPS by itself**, for the machine with no proxy in front: TLS through
    `rustls`, certificates from files or from Let's Encrypt, and HTTP/2 to
    browsers. Last, because the usual deployment does not need it.

`php_sapi_name()` answers `phera` under `serve`, as FrankenPHP answers
`frankenphp` and FPM `fpm-fcgi`; code that asks "is this the command line?"
tests for `cli` and keeps working.

Not planned: speaking FastCGI, that is, being a drop-in for PHP-FPM behind
an existing nginx `fastcgi_pass`. A proxy reaches Phera over HTTP like any
other upstream. This can be revisited if someone needs to keep an nginx
configuration unchanged.

## 6. Scope

In: the PHP that Symfony and Laravel and their dependencies run in production.

**Language, in scope:** namespaces, functions, closures, arrow functions,
first-class callables, named arguments, spread, references, static variables,
classes, interfaces, traits, enums, readonly, attributes, anonymous classes, late
static binding, magic methods, exceptions with `finally`, generators, `match`,
nullsafe, destructuring, alternative control syntax, heredoc and nowdoc,
`declare(strict_types=1)` with parameter and return type checks, `include` and
autoloading, `eval`, property hooks, asymmetric visibility, the pipe operator.

**Language, left out:** backticks, `(real)`, short and
ASP tags, `goto`, `declare(ticks)` and `__halt_compiler` data access.
`assert()` is checked, as under `php -n`: a failed assertion throws
`AssertionError` with the condition as written. A setting that compiles
assertions out, as PHP's production configuration does, comes with the
configuration file.
Fibers come after everything else works.

**Runtime, left out:** opcache and JIT settings, FFI, phar, pcntl, posix, the C
extension ABI, `dl()`, most of `php.ini` (a small fixed set of settings is
honoured), and functions removed or deprecated before PHP 8.

**Extensions, first wave:** standard (strings, arrays, math, output, variables,
files, `sprintf`, `serialize`), spl, json, pcre (bound to PCRE2), ctype, mbstring,
date, hash, filter, random, tokenizer, reflection, session, pdo with async
drivers. **Later:** openssl, curl, dom and xml, intl, fileinfo, iconv.

Functions are added on demand, driven by what failing tests and real framework
boots call, not by walking PHP's function list.

### 6.1 The yardstick: modern PHP 8.5, as one real application uses it

"Everything PHP does" is not the goal. The goal is modern PHP 8.5: what a
current application and its dependencies use. One reference application
decides what that means in practice: a Symfony 7.4 application with Doctrine
ORM 3 and Twig 3, tested with PHPUnit 12 and checked with PHPStan 2, about
2,700 PHP files of its own and 55,000 in `vendor/` (237 packages).

A scan of those files (PHP's own tokenizer, counting calls to built-in
functions and uses of built-in classes and syntax) gives the order of work.
State in November 2026:

| What the reference application uses | Used | Phera has |
| --- | --- | --- |
| Built-in functions (distinct) | 898 | 263 |
| Calls to built-in functions (sites) | 127,869 | 79,811 (62%) |
| Built-in classes and interfaces (distinct) | 231 | 28 |
| Uses of built-in classes (sites) | 32,890 | 15,112 (46%) |

What it needs most that Phera lacks, in order of use:

- **Language:** namespaces and `use` (every file), attributes (7,800 uses),
  `readonly` (5,000), generators (1,450 `yield`, 113 `yield from`), traits
  (415 declarations), enums (147), `match`, `?->`, first-class callables,
  asymmetric visibility. `goto` (35 uses) and `eval` (25) occur in
  dependencies, so they are in scope after all. The deprecated `"${name}"`
  and `"${expression}"` forms remain supported with PHP’s diagnostics.
- **Classes:** `JsonSerializable`, `ArrayAccess`, `Traversable`, `Iterator`,
  `IteratorAggregate`, `Countable` (engine interfaces); `DateTimeImmutable`
  and the date classes; `Generator`; the Reflection classes; `Redis`; the SPL
  containers and iterators; DOM and `XMLWriter`; the Intl formatters;
  `WeakMap`; `Fiber`.
- **Functions:** `class_alias`, `assert`, the file system and streams
  (`file_get_contents`, `fopen`, `is_dir`, ...), `extension_loaded`,
  `getenv`, `filter_var`, the `mb_*` and `hash*` families, `pack` and
  `unpack`, `random_bytes`, `serialize`, `parse_url`, `iterator_to_array`,
  `debug_backtrace`, output buffering, curl and sodium.
- **Packages:** only the autoload kinds `psr-4`, `classmap`, `files` and
  `exclude-from-classmap` occur. No package uses PSR-0, `target-dir` or
  `include-path`.

The scan is repeated when planning; a deprecated feature stays only if the
scan finds it in use.

### 6.2 Not supported means an error that says so

Deprecated and legacy things are not maintained. What Phera leaves out on
purpose must fail loudly and name the reason, never behave differently in
silence:

- **A function or class PHP 8.5 has and Phera leaves out** (for example
  `strftime()`, deprecated since PHP 8.1): calling it throws
  `Phera\NotSupportedException` (a `LogicException`) with the message
  "strftime() is not supported by Phera: deprecated since PHP 8.1".
  `function_exists()` and `class_exists()` answer false for these, so code
  that checks first takes its fallback. The list comes from the
  `#[\Deprecated]` attributes in PHP 8.5's own stub files, not from memory.
- **Syntax** (the backtick operator): a compile error, "... is not
  supported by Phera", with file and line.
- **Legacy package layouts** (PSR-0 autoloading, `target-dir`,
  `include-path`, PEAR-style trees): `phera install` stops and names the
  package and the field. Nothing is installed half.
- **`php.ini` settings** outside the honoured set: `ini_set()` returns false,
  as PHP does for an unknown setting.

Something merely not built yet is different: it keeps PHP's own error
("Call to undefined function ...") until it exists.

**Which deprecated items go, and which stay.** PHP 8.5.12 marks 148
functions, methods and constants with `#[\Deprecated]` in its stub files.
Each falls in one of three groups:

1. *Named items nothing uses*: they go, as described above. Examples:
   `strftime()`, `lcg_value()`, `utf8_decode()`, `mhash()`, `MT_RAND_PHP`,
   `FILTER_SANITIZE_STRING`, `assert_options()`.
2. *Named items the scan of 6.1 finds in use*, mostly calls that became
   no-ops and that current releases of widely used libraries still make:
   they stay, and behave as in PHP, deprecation message included. On
   2026-11-18 these were `curl_close()`, `curl_share_close()`,
   `imagedestroy()`, `xml_parser_free()`, `openssl_pkey_free()`,
   `libxml_disable_entity_loader()`, `utf8_encode()`, `ldap_connect()` with
   two arguments, `E_STRICT`, `DOM_PHP_ERR`, `DATE_RFC7231`,
   `NumberFormatter::TYPE_CURRENCY`, `setAccessible()` on reflection
   objects, `SplObjectStorage::attach()`, `detach()` and `contains()`, the
   driver constants on the `PDO` class (`PDO::MYSQL_ATTR_*` and relatives),
   and two pieces of syntax: `"${name}"` in strings and the long cast names
   (`(integer)`, `(boolean)`, `(double)`, `(binary)`).
3. *Deprecated behaviour of something supported*, which depends on the
   values at run time and so cannot be found by scanning (passing `null` to
   a built-in parameter that does not accept it, a float that loses
   precision as an array key, and so on): it stays as in PHP, with PHP's
   message. Turning these into errors would make maintained code fail on
   the data it happens to meet.

An item moves from group 2 to group 1 when a later scan no longer finds it.

## 7. Testing

Real PHP is the oracle at every layer.

| Layer | Check |
| --- | --- |
| Lexer | Token stream equals `token_get_all` over all of php-src |
| Parser | Parses every php-src test that `php -l` accepts |
| Values | Operator and conversion results equal real PHP over a generated matrix |
| VM and stdlib | `.phpt` tests: php-src's own, plus fixtures in `tests/fixtures/` |
| Server | HTTP integration tests against a running worker pool |

### 7.1 Taking tests from the PHP source

php-src has about 23,700 `.phpt` tests. `tests/phpt/sweep.sh` runs whole
directories of them, from the PHP 8.5.12 tag, with real PHP and with Phera,
and copies every test that passes on both into `tests/upstream/`, where it
becomes part of `make phpt` and so of every pull request's checks. A test
that real PHP 8.5.12 does not pass under the runner (it needs a section the
runner does not implement, or an extension that is not loaded) is never
taken. The list only grows: a selected test that stops passing is a
regression.

The same run is a measure of how far the engine is. Result on 17 November
2026 for `Zend/tests`, `tests`, `ext/standard/tests`, `ext/json/tests`,
`ext/pcre/tests` and `ext/ctype/tests` (10,488 tests): see the table in
`tests/upstream/README.md`, which the sweep rewrites. What fails is grouped by
its first line of output, which names the missing feature; those groups,
weighed by the usage scan in 6.1, are the worklist. Tests about deprecated
behaviour are not on it.

## 8. Milestones

| | Milestone | State |
| --- | --- | --- |
| 1 | **Frontend and foundations.** Lexer, parser, value model, HTTP server. | Done |
| 2 | **Core execution.** Bytecode, compiler and VM for procedural PHP. `phera file.php` prints output. | Done; a list of parity gaps is being closed |
| 3 | **Objects.** Classes, exceptions, closures, generators. Design in section 9. | Classes and exceptions done; closures in progress; then traits, enums, magic methods, generators |
| 4 | **Standard library.** Strings, arrays, math, ctype done (187 functions). Then pcre, mbstring, date, spl, output buffering, files. | First wave done |
| 5 | **Web.** `phera serve` runs scripts with superglobals and `header()`. Then uploads, cookies, sessions, autoloading. | Started |
| 6 | **Frameworks.** Reflection, sessions, PDO. Boot a bare Symfony kernel, then a Laravel hello world. | |
| 7 | **Snapshot and fork.** Phase 2 of request isolation. | |
| 8 | **Built-in tools.** `phera check` (a type checker) and `phera test` (a PHPUnit-compatible runner). Section 11. | Planned |

## 9. Objects, exceptions, closures, generators

The design for milestone 3. It is built in the order of the subsections,
each with its own fixtures checked against real PHP.

### 9.1 Classes and objects

- **Compile time.** A class declaration becomes plain data in the
  `CompiledUnit`: its name, parent and interfaces as symbols, flags
  (abstract, final, readonly, interface, trait, enum), constants, properties
  with their flags and default-value code, methods as ordinary `Function`s,
  enum cases. Nothing is resolved against other classes yet.
- **Run time.** The VM links a declaration into a `Class` when execution
  reaches it (or before the file runs, for a top-level class whose parent is
  already known, as PHP does). Linking copies the parent's tables, flattens
  traits in, and checks what PHP checks: abstract methods implemented,
  signatures compatible, no final overridden. A linked class is an
  `Rc<Class>` and never changes, except for its static properties.
- **Objects** stay `Rc<Object>` with a class handle and a property table, but
  the table moves behind methods (`get`, `set`, `unset`, `iter`) so its
  representation can change. It starts as the ordered map it is now, which
  gives `var_dump`, `foreach` and `(array)` their order for free. Slots
  indexed by declaration order, with a map only for dynamic properties, come
  later as an optimisation inside that one type.
- **Visibility.** Properties are stored under the keys PHP itself uses: the
  plain name for a public one, `\0*\0name` for a protected one and
  `\0Class\0name` for a private one. So a parent's and a child's private
  property of the same name coexist, and `(array)`, `var_dump` and
  `serialize` read the visibility from the key without asking the class.
  The compiler and VM know a declared property's visibility, so an access
  goes straight to the right key. Access checks compare the frame's class
  scope with the declaring class.
- **Calls.** A frame gains `this`, the class scope (for `self` and
  visibility) and the called class (for `static`). New instructions follow
  the existing call sequence: `InitMethodCall`, `InitStaticCall`, `New`,
  then the same `Send` and `Call`. Property access extends paths with
  `Step::Property`, so `$this->items[] = $x` is one copy-on-write walk like
  an array write.
- **Native classes.** The extension API gains `ClassEntry`: name, parent,
  interfaces, constants, properties, and methods as native functions that
  receive the object. `Throwable` and its family, `Closure`, `Generator`,
  `ArrayObject` and `stdClass` are defined this way.
- **Magic.** `__get`, `__set`, `__isset`, `__unset`, `__call`,
  `__callStatic`, `__toString`, `__invoke`, `__clone`, and the interfaces the
  engine itself understands (`ArrayAccess`, `Countable`, `Iterator`,
  `IteratorAggregate`, `Stringable`) are flags on the linked class, checked
  only on the slow path of an operation that failed the ordinary way.
- **Destructors.** Rust drops an object when its last reference goes, but a
  `Drop` cannot run PHP code: it has no VM and cannot await. So dropping an
  object whose class has `__destruct` moves its contents into a queue owned
  by the request. The VM empties the queue at the next instruction boundary
  and runs the destructors there. The timing is PHP's to within one
  instruction.
- **`__toString` and native functions.** A native function declared with a
  string parameter must accept a stringable object. The VM converts such
  arguments before `prepare_arguments`, because calling a PHP method needs
  the VM.

### 9.2 Exceptions

- `throw` raises an object. Errors raised by the engine and by native
  functions (`TypeError`, `ValueError`, ...) become instances of the same
  native classes, so PHP code can catch them.
- Unwinding is the one routine that exists today, extended to consult
  `Function::handlers`: innermost `try` first, frame by frame.
- A `catch` is compiled code: take the pending exception, test it with
  `instanceof` against each type, jump, or rethrow.
- A `finally` block is compiled once and entered as a subroutine. A register
  remembers how it was entered: by falling out of the `try`, by `return` (the
  value waits in a temporary), by `break` or `continue`, or by an exception
  (which continues unwinding afterwards). That is PHP's own scheme.
- Uncaught exceptions keep today's output. `set_exception_handler` and
  `set_error_handler` are VM intrinsics.

### 9.3 Closures and callables

- A closure's body is an ordinary `Function` with a list of captured
  variables, which occupy the registers after its parameters. Creating one
  is `MakeClosure` preceded by one capture instruction per variable, by value
  or by reference. An arrow function is the same with the captures worked
  out by the compiler.
- A closure value is an object of the native class `Closure` whose native
  state holds the function, the captured values, the bound `$this` and the
  class scope.
- One resolver turns any callable into a call target: a function name,
  `'Class::method'`, `[$object, 'method']`, a closure, an object with
  `__invoke`. `InitDynamicCall` and `Context::call` both use it, so
  `array_map` and `$f()` cannot disagree.
- `strlen(...)` creates a real `Closure`.

### 9.4 Generators

- Calling a generator function creates a `Generator` object instead of
  running the body. The object owns a frame that is not on the call stack.
- `yield` suspends: the frame's registers are moved out of the VM's register
  stack into the generator, and control returns to whoever resumed it.
  A scalar resume runs in that window. When an instruction needs the VM,
  the interpreter installs the same frame, with the receiver and pending
  call state restored, before executing the instruction.
- `foreach` advances a suspended generator in `IterNext` and reads the
  yielded value and any requested key together.
- `yield from` walks an explicit stack of inner generators. The outer
  generator keeps a weak link to a yielded leaf while that leaf can resume
  directly. A shared leaf that finishes or delegates again returns to the
  stack walk.
- A generator dropped while suspended inside a `try` runs its `finally`
  blocks through a request-local release queue. The VM drains it at an
  instruction boundary. A temporary that Phera still owns can delay that
  release compared with PHP. References kept by closures or reference
  cycles can also outlive the script's last explicit use of the generator.

### 9.5 Order of work

1. Native classes in the extension API; the object model; classes with
   properties, methods, constants, statics, inheritance, interfaces and
   abstract classes; `new`, `clone`, `instanceof`.
2. Exceptions.
3. Closures, arrow functions, callables.
4. Traits, enums, readonly, magic methods, the engine's interfaces.
5. Generators.

## 10. Speed

Measured on 17 November 2026 against PHP 8.5 without its JIT, with a clock
inside the script so that start-up is left out. Loops and arrays are 5 to 7
times slower than PHP. Anything that calls is 9 to 22 times slower.
(Earlier figures in this project used whole-process time and were too
favourable by a factor of two to four; see benchmarks/README.md.)

Waiting is where the design wins: one worker thread serves thousands of
requests that wait at the same time, where PHP needs a process for each.
See examples/concurrency.

A profile of method calls shows no single hot spot. The time is spread over
about forty small functions, because a call does far more bookkeeping than it
needs to. That is a design problem, and it is fixed by a simpler design, not
by tuning.

### 10.1 Rules for speed work

- Benchmarks keep their loops inside functions, since that is where real
  code runs, and cover different shapes: typed and untyped, ints, floats and
  strings, getters, constructors.
- No shortcut may recognise a particular sequence of instructions or a
  particular type spelling. A fast path must be general (a cache checked by
  identity, a bit test), and the benchmark must not be the only code it helps.
- Every claim is measured side by side against the previous commit, on
  scripts the author of the change did not write.

### 10.2 Step A: a call writes its arguments where the callee reads them

Today `InitCall` creates a pending-call record with its own argument list,
each `Send` pushes into that list, and `Call` then creates a frame and moves
the arguments into its registers, through a chain of helper records.

The new protocol, which is how PHP and Lua do it:

- `InitCall` resolves the callee (through the per-site cache) and reserves
  the callee's register window at the top of the register stack.
- Each `Send` writes the argument straight into its parameter's register in
  that window. A typed parameter is checked right there.
- `Call` only pushes the frame and continues at its first instruction.
  `Return` pops the frame and the window.
- A nested call inside an argument list (`f(g($x))`) reserves its window
  above the outer one, so the windows form a stack.
- The less common features build on the same window instead of a second
  protocol: a named argument writes to its parameter's register; spread
  writes several; extra arguments and a variadic parameter are collected
  from the registers past the declared parameters when `Call` runs;
  `func_get_args` and stack traces read the window.

### 10.3 Step B: fewer instructions per statement

`$sum += $p->add($i, 2)` inside a `for` loop is 17 instructions per round
where PHP needs about 8. Three causes, each a compiler rule with a VM
counterpart:

- A temporary is cleared by a separate `Clear` after the statement. Instead,
  the instruction that uses a temporary last takes it.
- An operator copies a local variable into a temporary before using it.
  Instead, operators read locals directly.
- Comparing and branching are fused only when one side is a literal.
  Instead, any comparison followed by a branch is one instruction.

### 10.4 Step C: beyond the interpreter

Only after A and B, because a faster way to run instructions gains little
while each call still costs what it costs now.

- A JIT (machine code generated at run time) cannot be written without
  `unsafe`: running generated code is unsafe by definition, and that code
  works on values behind raw pointers. PHP's own JIT gives roughly 1.5 to 3
  times on numeric loops and very little on web applications. It stays a
  proposal until A and B are measured.
- The larger gain for web applications is section 5: serving requests from
  an already loaded application instead of starting it for each request.

### 10.5 Data layout: the redesign behind the remaining gap

State on 17 November 2026, after step A and a rewrite of the simple-instruction
loop, timed inside the process: plain loops 2.2 to 2.8 times PHP, arrays
about 4 times, calls and objects 8 to 18 times.

The remaining gap on calls and objects has one cause. The VM finds things
by name at run time (a function, a method, a property, a class constant) in
hash tables keyed by strings, and then hides the cost behind caches: one per
call site, per property site, per static site, per unit. Each cache has a
guard to check, an owner to clone, and a miss path that duplicates the hit
path. The caches are the bulk of the VM's code and they are why a method call
costs 80 ns.

The rules from here on:

1. **Names become numbers once.** Every identifier is interned to a small
   integer when a unit is loaded. Nothing compares or hashes a string while
   PHP code runs, except for names that are computed at run time
   (`$obj->$name`, `call_user_func('f')`).
2. **Tables are flat arrays indexed by those numbers.** Functions, classes,
   methods, properties and constants live in `Vec`s. A reference to one of
   them is an index (`u32`), not a pointer with a reference count.
3. **Resolve once, at link time, to an index.** Loading a unit resolves what
   it can: a call to a declared function becomes "function number 17".
   What cannot be resolved until the code runs (a function declared later)
   is resolved on first use and written into a flat per-unit table, once,
   for good. That is linking, not caching: there is no guard and no
   invalidation, because a declared function or class never goes away.
4. **Objects are slots.** A class lays out its declared properties as slot
   numbers; a subclass keeps its parent's numbers and adds its own. An object
   is its class plus a flat array of values. `$this->x` in a method compiles
   to "slot 3", for this class and every subclass. Only undeclared (dynamic)
   properties live in a side table. Methods get the same treatment: a class
   has a method table whose slot numbers are stable in subclasses, so
   `$this->run()` is "method slot 5 of the receiver's class". A property or
   method on an object of unknown class is found through the class's small
   table from interned name to slot.
5. **No caches.** When a rule above replaces a lookup, the cache that hid
   the lookup is deleted, not kept as a fallback.
6. **A frame is a few numbers.** Which function (an index), where its
   registers start, where it continues, where its result goes. Whatever only
   some frames need (dynamic variable names, `foreach` state, a bound
   closure) lives in side tables keyed by frame, so a plain call does not
   pay for it. No reference count is touched to make a call.
7. **One `match` runs an instruction.** No chain of handler groups.
8. **Hot data is small.** A value is 16 bytes. Fields that are read on every
   instruction sit together; rarely used ones are elsewhere.

What this cannot do: the tightest arithmetic loop. A test interpreter in
plain safe Rust runs `$sum += ($i * 3) % 7` five million times in 29 ms
with every improvement applied (the loop test moved to the bottom, 16-byte
values, two operators fused); PHP takes 19 to 24 ms. Unchecked register
access (`unsafe`) would bring 29 down to about 28. PHP's remaining lead there
comes from how its C interpreter jumps between instructions, which stable
Rust cannot express. Closing that last 1.3 to 1.5 times on pure arithmetic
would take a JIT. On everything else there is no such limit.

Order: frames and dispatch (rules 6 and 7), functions and methods by index
(1 to 3), objects as slots (4), then the compiler work of 10.3, with rule 5
applied at each step.

### 10.6 Second opinion, and the order that follows from it

On 17 November 2026 Claude Opus reviewed the engine at commit cfed5d7 and
built a 400-line register VM in safe Rust to the rules of 10.5 (kept in the
scratch directory as `opus/proto/`). Independently, a 150-line model of one
method call loop was measured here. Both agree, and they change the plan.

**What a safe Rust interpreter with this layout reaches** (measured, in CPU
cycles, PHP 8.5 without JIT as 1.0):

| Shape | Prototype | Phera at cfed5d7 |
| --- | --- | --- |
| `fib(30)` | 1.3 | 7.7 |
| Method call in a loop | 1.2 | 10 |
| Objects created and used | 1.2 | 8.5 |
| Method call loop, the model here | 1.0 (15.8 ns against PHP's 16.1) | 12 (198 ns) |

So the design is right and the gap on calls is implementation, six to
twelve times of it. No `unsafe` is needed for any of that. Unchecked
register access would add 5 to 14 percent, most of which bytecode validated
at load plus fixed-size register windows can recover safely.

**What the review found that the plan did not have:**

1. *Build settings.* No release profile and the system allocator. Fat LTO,
   one codegen unit and mimalloc: 10 to 25 percent. Done in 54093d2.
2. *String array keys were copied on every access*, and the hash was
   SipHash. Done in 54093d2: 31 percent on a string-keyed loop.
3. *A call to a native function costs 313 cycles against PHP's 17.* Every
   native call leaves the synchronous loop, builds a 440-byte future, moves
   its arguments into a `Vec` and converts each one. Synchronous natives must
   run inside the loop, borrow their arguments from the register window, and
   skip conversion when a bit test says the types already fit. PHP compiles
   about twenty builtins (`strlen`, `count`, `is_*`, `in_array` with a
   literal array, ...) to instructions; Phera may do the same under the same
   conditions.
4. *Some property and array operations are forty times slower than PHP*:
   `$this->n += 1` costs 1,105 cycles (PHP 25) and `$this->a['x']++` 1,610
   (PHP 35), because every shape without its own fast path rebuilds the
   whole path with three allocations per property step. The remedy is one
   general walker that descends with `&mut Value` and allocates nothing,
   on top of objects as slots, not more fast paths.
5. *The compiler's optimiser skips every function that contains `try`*, misses
   obvious cases elsewhere, builds `[1, 2, 3]` with twelve instructions and
   string interpolation with one allocation per piece. Rules 10.3 will not
   hold as peephole passes. Expressions should be compiled with a
   destination in mind (`compile(expression, wanted place)`), which gives
   direct operands, consumed temporaries, fused branches and bottom-tested
   loops by construction, plus constant arrays as one constant and one
   `ConcatN` for a chain.
6. *Arrays*: a list should be a `Vec<Value>` without keys, a hashed array
   one entry list plus an index of positions. Measured 3.3 times on push and
   read.
7. *Strings*: two allocations each today. Short strings (up to 14 bytes) fit
   inside the 16-byte value.
8. *`Undef` as a value variant* instead of `Option<Value>` registers, so a
   register window can be handed to a native function as `&[Value]`.
9. *Callbacks* (`array_map`, `usort`) box two futures per element; natives
   that call back should return "call this, then resume me" to the loop.
10. *`foreach` by reference* re-resolves its place every round: 25 times PHP.
11. *Errors are 48 bytes*, so every `Result` in the handler chain travels
    through memory; a boxed error is 8.

**Corrections to 10.5.**

- The caches are about 6 percent of a call's cost, not the bulk of it. The
  cost is the protocol around the call: records cloned and dropped, a
  184-byte boxed frame reset per call, the dispatch chain. Deleting a cache
  speeds nothing up by itself; the first redesign attempt showed that
  (methods got 12 percent slower when the method cache was replaced by a
  lookup per call; it is kept on the branch `wip/engine-redesign-1`).
- Method slot numbers that stay stable in subclasses are not worth their
  special cases (interfaces, traits, private methods, `__call`). A flat
  per-class table from interned name to method costs about 1 ns per call.
  Stable slots remain the rule for properties.
- "No reference count is touched to make a call" cannot hold for a method:
  the receiver must be kept alive, as in PHP. The rule is "only the
  receiver's".
- A callee known at compile time needs no `InitCall` and no `Send`: the
  compiler evaluates the arguments into the top of the caller's window and
  emits one `CallDirect`. A declared function cannot be redeclared, so this
  is sound.

**Order of work from here.**

1. Calls and frames as in the prototype, with the instruction format for
   calls settled at the same time.
2. Native calls inside the loop; builtins as instructions.
3. Objects as slots and the allocation-free place walker, with interned names.
4. The destination-driven compiler.
5. Arrays, short strings, `Undef`, in the value crate; these are independent
   of 1 to 4 and can run beside them.
6. Callbacks, `foreach`, exceptions, class checks by index, boxed errors.

**Expected result**, from the prototypes: calls, methods and objects at 1.2
to 1.4 times PHP, tight integer loops at 1.2 to 1.5, arrays and short
strings at or near PHP. Beyond that only a JIT helps, and only for numeric
loops.

### 10.7 Item 1, calls: state after the first pass

Done on the branch `engine/calls` (pull request #9):

- A frame is `{ function, base, ip, destination, flags }`, 16 bytes in a
  `Vec`. What only some frames need (dynamic names, `foreach` state,
  argument facts, a closure, a class context) is in a side table by depth.
- One flat code table per request; functions, methods, closures and
  class initializers refer to code by `FunctionIndex`.
- One `match` executes an instruction. The interpreter loop borrows the
  code table, the call stack and the registers, and handles itself: simple
  instructions, `CallDirect`, plain returns (also of methods, closures and
  constructors), `InitMethodCall`, `InitStaticCall`, by-value `Send`s and
  `Call` when the case is the plain one.
- `CallDirect` replaces `InitCall`/`Send`/`Call` for calls to a file's own
  top-level functions (compiler pass `passes/direct_calls.rs`).
- Each class has a method table keyed by interned name. The per-site
  method cache and the static-site caches are deleted.

Measured on 17 November 2026 after merging `main` into the branch, in
instructions retired and cycles, each net of start-up (an empty script),
minimum of five runs, against `php -n` (load average 14 to 20, so cycles
are rough):

| script | start of the work | head before the merge | after the merge |
| --- | --- | --- | --- |
| fib | 8.0x (10.1x cycles) | 3.0x (3.1x) | 3.0x (3.0x) |
| hierarchy | 5.9x (7.0x) | 3.9x (4.6x) | 3.9x (4.5x) |
| methods | 7.0x (7.8x) | 5.7x (6.2x) | 5.7x (6.2x) |
| objects | 7.2x (10.9x) | 6.3x (9.0x) | 6.1x (8.3x) |
| closures | 7.4x (7.7x) | 7.2x (7.7x) | 7.1x (7.5x) |
| exceptions | 7.1x (8.1x) | 5.4x (5.8x) | 5.4x (5.5x) |
| calls | 13.5x (16.6x) | 12.3x (14.7x) | 12.6x (14.8x) |

No script is more than 2 percent worse than at the start in instructions
(typed-objects +1.0%, object-sort +1.2%, function-objects +1.6%); the
array scripts gained 4 to 10 percent from main's array storage.

Not reached: methods, hierarchy and objects at 3x and closures at 4x.
What remains in a method call is the general call record (an 88-byte
callee with two reference-counted signature parts and a 176-byte pending
call, built and dropped per call), the method context's three reference
counts (receiver, scope, called class as `Rc<Class>` rather than
indexes), `$this` kept in the side table rather than in register 0, and
about 35 machine instructions per simple bytecode instruction. Property
access in `objects` still leaves the loop (objects as slots, item 3).

### 10.8 Item 1, calls: the second pass

Done on the branch `engine/calls-2` (pull request #20):

- A call being prepared is a `PreparedCall`: which callee, where its
  window starts, how many arguments were sent and counted, its kind and a
  few flags. A call to PHP code (function, method, constructor, closure)
  carries nothing else; parameter names, by-reference flags, types and
  where a surplus argument goes are read from the callee's bytecode when
  the general path needs them. Only native callees and named arguments
  use a side entry (`CallDetails`).
- A call's kind says what its window holds below the first parameter:
  `$this` for a method, the closure object and its bound `$this` for a
  closure, whose captured values follow the parameters. Classes are
  numbered (`ClassIndex`) and the code table records each method's
  declaring class, so the class scope and the called class are derived:
  scope from the method (or the closure's binding), called class from
  `$this`, stored only for a static call whose called class is not the
  scope. A method call touches one reference count, the receiver's.
- In the loop: `new` (template allocation and the constructor), closure
  calls, `$this`, by-value and by-reference sends, defaults
  (`ReceiveDefault`, `LoadConst`, `CheckType`) and the call count of a call
  with defaults, kept in the frame's flags.

Measured on 17 November 2026 in instructions retired net of start-up
(cycles beside, minimum of five runs, load average 9 to 14), against
`php -n`:

| script | head of `engine/calls` | now | target |
| --- | --- | --- | --- |
| methods | 5.65x (6.6x) | 5.12x (5.1x) | 3x |
| hierarchy | 3.87x (4.6x) | 3.37x (3.7x) | 3x |
| closures | 6.94x (8.8x) | 6.43x (8.0x) | 4x |
| objects | 6.10x (9.1x) | 5.60x (8.4x) | 4.5x |
| fib | 2.98x (3.3x) | 3.09x (3.6x) | 2.5x |
| calls | 12.37x (16.0x) | 10.80x (14.2x) | 8x |

No target is reached, and `fib` is 3.7 percent worse in instructions than
at the head of `engine/calls` (the limit was 3). Every other script is
within 1 percent or better.

Why, from a line-level profile of `$x = $o->m()` in a loop (1,151 machine
instructions per round against PHP's 212): about 40 percent is the
simple-instruction loop (`scalar_step`; the compiler emits two to three
times PHP's instructions per statement, and each costs about 46 machine
instructions against PHP's 25 to 30), the rest is spread over preparing
the window, the frame push and pop, emptying the callee's registers and
the visibility checks, with no single hot spot. The code generated for the
loop is sensitive to layout: moving rarely used steps out of line, or
restructuring the method preparation, changed scripts that never run that
code by 5 to 10 percent in either direction. In `objects`, property access
(which still leaves the loop until objects are slots) is 2,918 of 6,458
machine instructions per round; without it the script would be about
3.9x.

What remains, in order of expected gain: the destination-driven compiler
(10.6 item 4) for fewer instructions per statement; objects as slots (item
3) for `objects` and `calls`; native calls in the loop (item 2) for
`closures`, whose `array_map` part calls back from native code.

## 11. Tools built into the runtime

Two tools that PHP projects today get from separate packages written in PHP
are planned as part of the `phera` binary, sharing its lexer, parser and AST.
One front end means the tools and the runtime cannot disagree about what a
file means, and native code makes them fast enough to run on every save.

### 11.1 `phera check`: a type checker

What PHPStan and Psalm do: find errors without running the code.

- **Pipeline.** Find the files; parse them in parallel (the parser is pure,
  so this scales with cores); collect every declaration (classes,
  interfaces, traits, enums, functions, constants, with their signatures)
  into one index for the project; then analyse each function body: track the
  type of every variable through the control flow, infer the type of every
  expression, and let rules report what cannot be right.
- **Type model.** Keep scalar facts inline and share immutable compound types
  across scopes and declarations. A literal such as `3` is narrower than
  `int`, and `never` is the empty set of values. A join records the values
  possible on either branch. An intersection such as `Readable&Seekable`
  retains members from both bounds. Class ancestry comes from the lazy
  declaration index. Arrays keep key and value types, lists keep sequential
  integer keys, and shapes also record named and optional keys. Callable
  types carry their parameter and return signatures. Iterables retain key
  and value types even when supplied by a class.
- **Declaration types.** Keep a declaration's native type and its PHPDoc
  refinement separately. For example, `@param list<User> $users` refines
  `array $users`, and an incompatible doc type is a diagnostic on the tag.
  Parse the PHPDoc grammar independently of PHP syntax, with bounded
  nesting, namespace imports, tag precedence, and source locations. Use
  declared template bounds and infer simple template arguments at calls.
  Resolve `self` against the declaring class, `static` against the receiver,
  and `$this` against the receiver identity. A class-string carries its
  class bound. Preserve literal ranges and other refinements when the
  checker knows what they mean. Erase an unsupported pseudo-type to the
  nearest type that can be justified by its syntax.
- **Flow and compatibility.** Expression evaluation supplies a type and
  sends facts to the true and false scopes. A join keeps facts supported
  by both paths and can retain an implication tied to an unchanged
  variable, such as `$ok === true` implying that `$value` was assigned.
  Narrowing records the condition's location so diagnostics can identify
  where a fact came from. Native acceptance considers the operation and
  the relevant `strict_types` setting. PHPDoc acceptance compares the
  refined types. Keep unknown acceptance distinct from a proven mismatch
  so that these levels can analyse untyped code.
  For example, a known `string` returned from an `int` function is a
  level 3 mismatch. A weak return context permits a `Stringable` object
  when the function returns `string`. A callback that can write a captured
  property invalidates earlier facts about that property.
- **Analysis concurrency.** Composer resolution and declaration linking
  precede each analysis pass. Workers analyse independent files against a
  shared index and return unresolved class requests alongside their
  results. A subsequent pass resolves new requests and revisits affected
  files. Diagnostic ordering is independent of worker completion order.
- **Composer projects.** Check paths such as `src` or `app routes` from a
  directory with `composer.json` and `vendor/composer`, or pass paths
  inside that project from elsewhere. Phera parses Composer's generated
  classmap, PSR-4 map and files map without executing PHP. It reads
  autoload files for helper declarations and loads classes and their
  ancestry as references are encountered. PSR-0 is unsupported.
  Dependency declarations inform checks, and diagnostics are limited
  to the requested files. Passing a path explicitly inside `vendor`
  checks that path too.
- **Project paths.** In `phera.toml`, `[check].paths` selects what to
  report on. `scan = ["legacy/lib"]` indexes additional directories
  without reporting on them, as with PHPStan's `scanDirectories`.
  `exclude = ["src/generated"]` leaves files or directories out of
  discovery in both lists, as with PHPStan's `excludePaths`.
- **Knowledge of PHP itself** comes from php-src's own `.stub.php` files,
  parsed by Phera's parser at build time into a compact table. So the checker
  knows every built-in function and class of PHP, not only those the runtime
  implements so far.
- **Levels.** Checks accumulate. Levels 0 and 1 cover basic symbol checks,
  count checks, and variable definition. Level 2 reads and validates
  PHPDoc and checks members of known receiver types. Level 3 checks
  returned values and typed property writes. Level 4 checks provably
  constant predicates and unreachable code. Level 5 checks argument types
  against resolved signatures. A call to `strlen(...)` has the same
  parameter type as a direct call to `strlen`.
- **Output**: a readable report, JSON, and annotations for CI; a baseline
  file to adopt it on an existing code base; `@phpstan-ignore` and
  `@psalm-suppress` comments are honoured so existing code needs no edits.
  Native union names use PHP's display order through the bytecode crate,
  so `int|string` is displayed as `string|int` in both runtime and checker
  diagnostics.
- **How it is verified**: differentially. A corpus of small PHP files, each
  with one deliberate error or none, is run through PHPStan at the matching
  level and through `phera check`; both must flag the same lines. Then real
  open-source projects, comparing the reports.
- **Extension points.** PHPStan plugins and inference beyond declared
  templates with simple call-site substitutions require separate work.

### 11.2 `phera test`: a test runner

Existing PHPUnit tests should run unchanged.

- **Compatible surface**: `PHPUnit\Framework\TestCase` with `setUp`,
  `tearDown` and their class-level forms; the assertions; `expectException`
  and friends; data providers; tests found by the `test` prefix and by
  attributes; `phpunit.xml` for suites and bootstrap; PHPUnit's exit codes,
  a familiar progress display, and JUnit XML for CI.
- **Native**: the assertions are native methods; discovery and scheduling
  belong to the runtime. Every test class runs in its own fresh VM as a
  task, so tests are isolated from each other without extra processes and
  run in parallel on all cores by default.
- **Needs first**: namespaces with `use`, autoloading (Composer's PSR-4 and
  class map), attributes, traits. So it follows those engine features.
- **Later**: mock objects (`createMock`), which need classes generated at
  run time.
- **How it is verified**: the same test suites under real PHPUnit and under
  `phera test` must give the same result per test.

### 11.3 Order of feature work towards "it all matches"

The measure stays the same: the same script, the same output as PHP 8.5.

1. Magic methods (`__toString`, `__get`, `__set`, `__isset`, `__unset`,
   `__call`, `__callStatic`, `__clone`, `__destruct`), and the engine's
   interfaces (`Stringable`, `Countable`, `ArrayAccess`, `Iterator`,
   `IteratorAggregate`, `Traversable`, `JsonSerializable`).
2. Traits, enums, readonly classes, first-class enum functions.
3. Namespaces end to end, autoloading, `include` paths, Composer.
4. Attributes and reflection, as far as frameworks and PHPUnit use them.
5. Generators.
6. The standard library in breadth: date and time, mbstring, files and
   streams, output buffering, hashing, serialization, SPL.
7. Sessions, cookies, uploads for the web side.

Items 1 to 5 change the engine and wait for the call redesign of section 10
to land, so that they are built on the new call path and not twice. Item 6
is independent of the engine and runs beside it.

### 11.4 Extensions: what ships in the binary

In PHP, an extension is a C module that has to be compiled for your PHP
version and enabled in an ini file, which is why "is mbstring installed?" is
a real question on every server. In Phera, the extensions people actually
use are part of the binary: one download, nothing to install or enable.

- **Every extension is a crate** under `crates/extensions/`, with the same
  shape: it registers functions, constants and classes through the extension
  API and knows nothing about the VM. The runtime loads all of them.
- **Each stands on the best Rust crate for its job** rather than on the C
  library PHP wraps, so the build needs no system packages: PCRE2 (bundled
  source) for `preg`, `tokio-postgres` and `mysql_async` for PDO,
  `encoding_rs` and the Unicode crates for mbstring, the RustCrypto crates for
  hashing. A C dependency is accepted only where nothing else gives
  PHP-exact results.
- **Anything that waits is asynchronous**: database queries, HTTP calls,
  file and network I/O, password hashing. That is the property a C extension
  cannot offer and the reason to write these again.
- **Cargo features** let an embedder leave extensions out; the default build
  has them all.
- **`extension_loaded()`, `get_loaded_extensions()` and `function_exists()`
  tell the truth**, so code that probes for an extension works.

The list, in the order of work. Shipped: core, standard (strings, arrays,
math, time), ctype, json, pcre, pdo with pdo_pgsql and pdo_mysql. In
progress: mbstring and iconv, date, hash, random. Next: files and streams,
output buffering, serialization, URL and filter functions, session, curl
(an asynchronous HTTP client behind PHP's curl API), openssl and sodium, zlib,
SPL, tokenizer, fileinfo, bcmath and gmp, intl, the XML family (dom,
simplexml, xmlreader, xmlwriter), gd, zip, sqlite, redis.

Third-party extensions written in C cannot be loaded, since there is no
Zend API to load them against. An extension someone needs is ported as a
crate against the extension API.

### 11.5 `phera install`: packages, as fast as the JavaScript and Python tools made it

Composer is written in PHP, resolves dependencies on one core, downloads
with limited parallelism and unpacks every package into every project. pnpm,
Bun and uv showed what the same job looks like when it is done natively:
resolve in parallel, download everything at once, keep one copy of each
package version on the machine and link it into projects. `phera install`
applies that playbook to PHP packages, and stays compatible with Composer so
that a project can use either tool.

- **Same files.** It reads `composer.json`, reads and writes `composer.lock`
  in Composer's format, and installs into `vendor/` with a Composer-compatible
  `vendor/autoload.php`, so the project still runs under plain PHP and
  Composer can take over at any time. No new manifest format.
- **Same registry.** Packagist's metadata API and the dist archives it points
  to; also `repositories` entries of type `composer`, `vcs`, `path` and
  `artifact`, and private registries with `auth.json`.
- **Same answers.** Composer's version constraints (`^`, `~`, ranges,
  stability flags, branch aliases), `replace`, `provide`, `conflict`,
  `minimum-stability`, `prefer-stable`, and platform requirements (`php`,
  `ext-*`), which are checked against what Phera really has. The resolver is
  PubGrub (the algorithm uv uses), which is fast and explains a conflict in
  plain sentences.
- **A global store.** Every package version is downloaded and unpacked once
  per machine into a content-addressed store and placed into `vendor/` with
  copy-on-write clones or hard links, so a second project with the same
  dependency costs no download and almost no disk.
- **Parallel everywhere.** Metadata is fetched concurrently while the resolver
  runs, archives are downloaded and unpacked concurrently, the autoloader's
  class map is built by scanning files on all cores with Phera's own parser.
- **Installing from a lock file needs no resolution** and, when the store is
  warm, no network: it is linking files.
- **Commands**: `phera install`, `phera update`, `phera add`, `phera remove`,
  `phera why`, `phera outdated`, `phera audit` (security advisories from
  Packagist), `phera dump-autoload`, and `phera x vendor/bin-tool` to run a
  package's binary.
- **What does not carry over**: Composer plugins (they are PHP code that
  hooks into Composer's internals). The few that matter are handled natively
  (the installers for framework-specific paths); `scripts` in `composer.json`
  do run, through Phera.
- **Verification**: differential, against Composer itself. For a set of real
  projects' `composer.json` files: installing from their lock file must
  produce the same `vendor/` tree (same files, same autoloader behaviour),
  and resolving from scratch must produce the same versions, or a documented
  reason for each difference. Timings are measured cold and warm against
  Composer.

Legacy layouts are not installed (6.2): a package that declares PSR-0
autoloading, `target-dir` or `include-path` stops the install with an error
naming the package and the field.

The runtime side: when a script runs under Phera, autoloading reads the same
metadata natively instead of executing the generated PHP autoloader, which
needs namespaces and autoloading in the engine (11.3, item 3).

### 11.6 Phera views: a native template renderer

Frameworks return HTML through a template layer: plain `.php` views in
CodeIgniter, Blade in Laravel, Twig in Symfony. Blade and Twig are compilers
written in PHP that turn templates into PHP classes; they escape for HTML
text by default and leave every other context (an attribute, a URL, a script
block) to the author. A runtime can do this job itself, and do three things
better: choose the escaping from where the value lands, forbid side effects
in a view, and write the page without building strings.

Working name: a view is a file ending in `.phview`.

```php
<?php /** @var list<Product> $products */ ?>
<ul class="<?= $listClass ?>">
<?php foreach ($products as $product): ?>
  <li><a href="/p/<?= $product->slug ?>"><?= $product->name ?></a></li>
<?php endforeach ?>
</ul>
<script>const count = <?= count($products) ?>;</script>
```

- **The syntax is PHP's own template syntax** (`<?= ?>`, `foreach:` /
  `endforeach`), so nothing new has to be learned and the existing parser
  reads it. What differs is what the compiler does with it.
- **Escaping by context, decided at compile time.** The compiler runs an HTML
  tokenizer over the literal text, so it knows that the first `<?= ?>` above
  is inside a quoted attribute, the second inside a URL path, the third in
  text and the fourth inside a script. Each gets the escaper for its place
  (the model of Go's `html/template`). A value in a place that cannot be
  escaped safely (an unquoted attribute, a tag name, an event-handler
  attribute) is a compile error. Output that must not be escaped has to be a
  `Phera\View\Html` value, which only a view or an explicit
  `Phera\View\trusted()` call produces, so every bypass is greppable.
- **A view cannot do anything but render.** It reads the variables it was
  given, calls pure built-ins (string, array, number and date formatting) and
  helpers the application registered, includes other views and fills slots of
  a layout. Assignments to anything but its own locals, `include` of code,
  database, file and network functions, `eval` and superglobals are compile
  errors. This is checked on the syntax tree, so it costs nothing at run time,
  and it makes a view safe to hand to a designer.
- **No string building.** A view compiles to the same bytecode as any
  function. Its literal text is static bytes that are written to the response
  as they are; values are escaped straight into the output buffer. The page
  is streamed, so the browser has the `<head>` while the body is still being
  produced. Compiled views are shared by all workers, like any compiled file.
- **From PHP:** `Phera\View\render('products/list', ['products' => $p])`
  writes to the response; `Phera\View\capture(...)` returns the `Html`
  value. A Twig or Blade application keeps working without it: those engines
  are PHP code and run as such.
- **`phera check` reads views too**: the `@var` line declares what a view
  expects, and a controller passing the wrong shape is a finding.

Order: the engine comes first (6.1). The parts that do not depend on it, the
HTML context tokenizer and the escapers, can be built and tested before that.

### 11.7 Files and streams

The largest family of functions still missing: `file_get_contents`,
`file_exists`, `fopen` and its relatives are called 4,000 times in the
reference application's code and packages, and 600 php-src tests that real
PHP passes stop at them.

- **One stream type.** A stream is a Rust object behind a PHP resource:
  `file://` (plain files), `php://memory` and `php://temp`, `php://stdin`,
  `stdout`, `stderr`, `php://input` and `php://output`, `data:`. `http://` and
  `https://` come with the HTTP client; user wrappers
  (`stream_wrapper_register`) after that. Everything else in PHP's wrapper
  list is left out (6.2).
- **What may wait runs off the worker thread.** A worker thread serves many
  requests, so a call that can take long must not hold it: reading or writing
  more than 64 KiB, directory scans, `flock` that has to wait, pipes, terminals
  and sockets go through Tokio's file and blocking facilities, and the script
  is suspended meanwhile.
- **What cannot wait stays on the thread.** `file_exists`, `is_file`, `stat`,
  `filemtime`, `realpath` and small reads of local files answer in about a
  microsecond; a hop to another thread and back costs twenty times that, and
  an autoloader makes thousands of these calls per request. They run where
  they are called, as in PHP. A setting sends them through the pool as well,
  for projects on network file systems.
- **No stat cache.** PHP caches the last `stat` result and has
  `clearstatcache()`; frameworks call it defensively. Phera asks the
  operating system every time, and `clearstatcache()` does nothing.
- **Per request.** Open streams belong to the request and are closed when it
  ends. The working directory (`chdir`, `getcwd`) and `umask` are per request
  too, kept as values and applied to each path, because the process's own are
  shared by every request.
- **Confinement.** Under `serve`, an optional list of directories outside
  which no file function reads or writes (PHP's `open_basedir`, checked on the
  resolved path).

### 11.8 `phera new`: start a project

One command starts a project, the way `npm create vite` does for a front end:
it asks for a name and a kind of project, and what it is not asked on the
command line it asks on the terminal.

```
phera new                                   asks for both
phera new shop --template symfony           asks nothing
```

| Template | What it makes |
| --- | --- |
| `plain` | A small JSON application without packages: a front controller, `src/` with a request, a response and the routes, `composer.json`, `.env`. The files are built into the binary (`crates/cli/templates/new/`). |
| `symfony` | Symfony's official skeleton, 7.4 (the long-term support line, 6.1). |
| `symfony-webapp` | The skeleton plus the `webapp` pack: Twig, Doctrine, forms, security. |
| `laravel` | The official Laravel application. |

- **Frameworks come from their own starting projects**, fetched with
  Composer, not from copies kept in Phera: a copy would be out of date with
  the next release of the framework. `phera install` (11.5) takes over from
  Composer when it exists; until then a framework template needs Composer on
  the machine, and the plain template only uses it to write the class loader.
- **It never overwrites.** The directory must be missing or empty.
- **The name** is the last part of the directory. It is put into PHP, JSON
  and Markdown, so only letters, digits, `-`, `_` and `.` are accepted.
- **Without a terminal** (a script, a pipeline) an unanswered question is an
  error that names the options, not a wait.
- **While Phera cannot run a framework**, the command says so after creating
  the project and names the PHP command that serves it meanwhile. One
  function, `Template::runs_on_phera`, holds that fact.

## 12. What to take from Hack and HHVM, and what not

Facebook's HHVM ran PHP much faster, then became the runtime of a different
language, Hack. Both halves of that story are useful.

**Worth taking, as implementation ideas:**

- **Whole-program mode.** HHVM's "repo authoritative" mode compiles the whole
  application once, at deploy time, and then refuses anything that would
  change it (`eval`, includes computed at run time). With the whole program
  known, every function, class and constant is resolved before the first
  request, class layouts are final, and calls and property accesses compile
  to indexes. Phera's link step (10.5) is the lazy version of this; a
  `phera build` that produces one resolved image for production is the
  eager one, and fits section 5 (start requests from a loaded image).
- **Types that drive the compiler.** HHVM infers types across the whole
  program and emits specialised bytecode. Phera will have the same
  information from `phera check` (11.1), in the same binary, on the same
  AST: where the checker proves that `$i` is an int and `$this->items` a
  list, the compiler can emit the integer and list instructions directly.
  No other PHP runtime has its checker and its compiler in one place.
- **Lists and maps as different things.** Hack split PHP's array into `vec`,
  `dict` and `keyset`. Phera does this inside the runtime (a list stores no
  keys, 10.6) and the checker can tell the programmer which one a variable
  is, without changing the language.
- **A checker that is a service.** Hack's checker separates collecting
  declarations from checking bodies, checks files in parallel, and keeps
  running to re-check only what an edit touched. That is the right shape for
  `phera check` and for editor integration.
- **Awaiting several things at once.** Hack has `async`/`await`. Phera's
  model is simpler for existing code (PHP stays synchronous and the runtime
  suspends it), but Hack shows the missing piece: running several waits
  concurrently inside one request (two queries, three HTTP calls). That
  deserves a small native API later.

**Not to take:**

- **Forking the language.** Hack removed references, variable variables,
  dynamic properties and more, and gained its speed and its type system
  partly from that. It also lost the PHP ecosystem: frameworks and packages
  stopped running, and HHVM dropped PHP support in 2019. Phera's whole
  premise is that existing PHP runs unchanged. Dynamic features stay, and
  cost only the code that uses them (a function that uses `extract` gets a
  slower frame; the others do not).
- **A JIT first.** HHVM's JIT is years of work by a large team. Section 10
  shows that an interpreter with the right data layout gets within a small
  factor of PHP's speed; the JIT question comes after that.
