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

Measured in instructions retired against PHP 8.5 (cycles were unreliable
at load averages of 10 to 33): fib 8.0x -> 3.0x (2.15x in cycles at a
calmer moment), hierarchy 6.0x -> 4.0x, methods 7.0x -> 5.7x, objects
7.2x -> 6.3x, calls 13.5x -> 12.3x; no script more than 2 percent worse.

Not reached: methods, hierarchy and objects at 3x and closures at 4x.
What remains in a method call is the general call record (an 88-byte
callee with two reference-counted signature parts and a 176-byte pending
call, built and dropped per call), the method context's three reference
counts (receiver, scope, called class as `Rc<Class>` rather than
indexes), `$this` kept in the side table rather than in register 0, and
about 35 machine instructions per simple bytecode instruction. Property
access in `objects` still leaves the loop (objects as slots, item 3).

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
- **Types.** A small lattice: the scalar types, literal values, `null`,
  unions, class names with their ancestry, `list<T>` and `array<K, V>`,
  array shapes, `callable` signatures, `mixed`, `never`. Declared native
  types are facts; everything else is inferred.
- **Knowledge of PHP itself** comes from php-src's own `.stub.php` files,
  parsed by Phera's parser at build time into a compact table. So the checker
  knows every built-in function and class of PHP, not only those the runtime
  implements so far.
- **Levels**, as in PHPStan, so a project can adopt it gradually. Level 0:
  unknown classes, functions, methods on `$this`, constants; wrong argument
  counts; variables that are certainly undefined. Then: possibly undefined
  variables; methods and properties on any expression whose type is known;
  return and property types; dead code and comparisons that are always true
  or false; argument types; and from there PHPDoc types (`@param`,
  `@return`, `@var`, generics, shapes).
- **Output**: a readable report, JSON, and annotations for CI; a baseline
  file to adopt it on an existing code base; `@phpstan-ignore` and
  `@psalm-suppress` comments are honoured so existing code needs no edits.
- **How it is verified**: differentially. A corpus of small PHP files, each
  with one deliberate error or none, is run through PHPStan at the matching
  level and through `phera check`; both must flag the same lines. Then real
  open-source projects, comparing the reports.
- **Not in the first versions**: a plugin interface, and full parity with
  PHPStan's generics inference.

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

The runtime side: when a script runs under Phera, autoloading reads the same
metadata natively instead of executing the generated PHP autoloader, which
needs namespaces and autoloading in the engine (11.3, item 3).

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
