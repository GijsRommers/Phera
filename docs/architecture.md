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

## 6. Scope

In: the PHP that Symfony and Laravel and their dependencies run in production.

**Language, in scope:** namespaces, functions, closures, arrow functions,
first-class callables, named arguments, spread, references, static variables,
classes, interfaces, traits, enums, readonly, attributes, anonymous classes, late
static binding, magic methods, exceptions with `finally`, generators, `match`,
nullsafe, destructuring, alternative control syntax, heredoc and nowdoc,
`declare(strict_types=1)` with parameter and return type checks, `include` and
autoloading, `eval`, property hooks, asymmetric visibility, the pipe operator.

**Language, left out:** backticks, `${expr}` interpolation, `(real)`, short and
ASP tags, `goto`, `declare(ticks)`, `__halt_compiler` data access, and
`assert()` with side effects (assertions compile to nothing, as in production).
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

## 7. Testing

Real PHP is the oracle at every layer.

| Layer | Check |
| --- | --- |
| Lexer | Token stream equals `token_get_all` over all of php-src |
| Parser | Parses every php-src test that `php -l` accepts |
| Values | Operator and conversion results equal real PHP over a generated matrix |
| VM and stdlib | `.phpt` tests: php-src's own, plus fixtures in `tests/fixtures/` |
| Server | HTTP integration tests against a running worker pool |

## 8. Milestones

| | Milestone | State |
| --- | --- | --- |
| 1 | **Frontend and foundations.** Lexer, parser, value model, HTTP server. | Done |
| 2 | **Core execution.** Bytecode, compiler and VM for procedural PHP. `phera file.php` prints output. | Done; a list of parity gaps is being closed |
| 3 | **Objects.** Classes, exceptions, closures, generators. Design in section 9. | Next |
| 4 | **Standard library.** Strings, arrays, math, ctype done (187 functions). Then pcre, mbstring, date, spl, output buffering, files. | First wave done |
| 5 | **Web.** `phera serve` runs scripts with superglobals and `header()`. Then uploads, cookies, sessions, autoloading. | Started |
| 6 | **Frameworks.** Reflection, sessions, PDO. Boot a bare Symfony kernel, then a Laravel hello world. | |
| 7 | **Snapshot and fork.** Phase 2 of request isolation. | |

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
  `current`, `next`, `send` and `throw` move them back and continue. This is
  why frames were kept detachable from the start.
- `yield from` keeps a stack of inner generators inside the outer one.
- A generator dropped while suspended inside a `try` runs its `finally`
  blocks through the destructor queue.

### 9.5 Order of work

1. Native classes in the extension API; the object model; classes with
   properties, methods, constants, statics, inheritance, interfaces and
   abstract classes; `new`, `clone`, `instanceof`.
2. Exceptions.
3. Closures, arrow functions, callables.
4. Traits, enums, readonly, magic methods, the engine's interfaces.
5. Generators.
