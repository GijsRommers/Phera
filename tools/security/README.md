# Security fuzzing tools for the Phera engine

Defensive, authorized security testing of the engine: hostile PHP source and
hostile data fed to `phera` (the CLI runner and the `--dump` front end). The
HTTP server and PDO are out of scope for this harness.

Everything here is safe Rust (no `unsafe`), std-only, and lives outside the
main workspace so it never becomes a workspace member.

## Build

```sh
cd tools/security
cargo build --release          # produces target/release/phera-fuzz
```

Build the engine first: `cargo build --release -p phera-cli` from the repo
root (on this machine set `SDKROOT` to a working SDK, e.g.
`SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.sdk`).

## The binary

```
phera-fuzz seeds   --root <dir>... --corpus <dir>     extract --FILE-- sections of .phpt files + .php/.inc files
phera-fuzz mutate  --phera <bin> --corpus <dir> --out <dir> [--seconds N] [--jobs N] [--timeout-ms N] [--seed N]
phera-fuzz dump    same options as mutate, but runs `phera --dump` (front end only: lexer, parser, compiler)
phera-fuzz gen     --phera <bin> --out <dir> ...        structure-aware generator (nesting, chains, recursion, decoders)
phera-fuzz funcs   --phera <bin> --out <dir> ...        hostile arguments for ~110 native functions x ~50 argument shapes
```

Common options: `--mode run|dump`, `--seconds` (campaign length, default 600),
`--jobs` (parallel workers, keep at 2: the machine is shared), `--timeout-ms`
(per-input time limit, default 5000), `--seed` (RNG seed, for reproducibility).

## Running a campaign (resource limits)

Campaigns must run at low priority, on at most 2 cores, with every target
process under a time and memory limit. The runner enforces the limits itself:
each input is executed with a wall-clock timeout and its resident memory is
polled (`ps -o rss=`) and the process is killed above 2 GiB RSS or above a
512 MiB output cap. Start the fuzzer niced so all children inherit it:

```sh
SCRATCH=/path/to/scratchpad/security-engine
PHERA=$PWD/target/release/phera
FUZZ=$PWD/tools/security/target/release/phera-fuzz

# 1. seed corpus from the repo's own tests
$FUZZ seeds --root tests/upstream --root tests/fixtures --corpus $SCRATCH/corpus

# 2. source mutation campaign (the long one; 2 h+)
nice -n 15 $FUZZ mutate --phera $PHERA --corpus $SCRATCH/corpus \
  --out $SCRATCH/campaigns/mutate-run --seconds 7200 --jobs 2 --timeout-ms 5000 --seed 4242

# 3. front-end-only campaign (lexer/parser/compiler, nothing executes)
nice -n 15 $FUZZ dump --phera $PHERA --corpus $SCRATCH/corpus \
  --out $SCRATCH/campaigns/mutate-dump --seconds 1800 --jobs 2 --timeout-ms 10000 --seed 777

# 4. structure-aware generator
nice -n 15 $FUZZ gen --phera $PHERA --out $SCRATCH/campaigns/gen --seconds 1800 --jobs 2 --seed 99

# 5. hostile native-function arguments
nice -n 15 $FUZZ funcs --phera $PHERA --out $SCRATCH/campaigns/funcs --seconds 1800 --jobs 2 --seed 5150
```

To run them detached (so they survive the tool call that started them):

```sh
python3 tools/security/detach.py run.log run.pid bash -c \
  "nice -n 15 $FUZZ mutate --phera $PHERA --corpus $SCRATCH/corpus --out $SCRATCH/run1 --seconds 7200 --jobs 2"
```

(`detach.py` is a small helper that `fork`+`setsid`s the command;
`nohup ... &` does not survive in this environment.)

## Reading the output

Each campaign writes `<out>/progress.log`:

- `PROGRESS executed=N clean=N findings=N` every 30 s — liveness and counts.
- `FINDING <kind> sig=<signature> new=<bool> file=<path> note=<what was done>`
  for every input that crashed, hung, or blew a limit. `kind` is one of
  `signal` (killed by a signal: abort/segv/...), `panic` (panic text on
  stderr), `timeout`, `memory` (RSS > 2 GiB), `output` (> 512 MiB printed).
  `new=false` means the signature was seen before (deduplicated).
- `DONE executed=... clean=... findings=...` at the end.

Every finding is saved as `<out>/findings/case-<timestamp>.php` plus a
`.meta.txt` sidecar with the exact command, outcome, elapsed time, peak RSS,
stdout size, the tail of stderr, and the mutation/generator note. Re-run any
finding with:

```sh
$PHERA <out>/findings/case-<timestamp>.php        # or: $PHERA --dump <file>
```

and compare with real PHP: `/opt/homebrew/bin/php -n <file>`.

## What each campaign covers

- **mutate**: byte flips/inserts/deletes, slice duplicate/delete/splice from
  another seed, token duplicate/delete/insert/swap, number-literal replacement
  with boundary values (`PHP_INT_MAX`, `9223372036854775808`, `NAN`, `INF`,
  ...), identifier replacement, and wrapping a random token-aligned slice in
  `if`/`try`/`array()`/parens/closure/list. Seeds are the `--FILE--` sections
  of all `tests/upstream/**/*.phpt` and `tests/fixtures/**/*.phpt` (~2500
  files).
- **dump**: the same mutations, but only `phera --dump` runs — no bytecode
  executes, so lexer/parser/compiler crashes are found fast.
- **gen**: one template per risk, at scales 8..100000 — nesting depth of
  parens/arrays/ternaries/closures/try/match/interpolation/class-extends/if,
  long operator and method chains, long strings and identifiers, huge literal
  argument lists, deep PHP recursion (plain, mutual, through closures,
  through `array_map`), deep/cyclic structures built then freed or dumped,
  exceptions thrown from arguments/`finally`/destructors/`__toString`/
  generators/deep unwind, and hostile decoder input (`json_decode` depth,
  malformed UTF-8, catastrophic `preg_*` patterns, `sprintf` widths, huge
  `str_repeat`/`str_pad`/`range`/`array_fill`, integer-overflow and
  comparison semantics, array-key normalisation, visibility/readonly/
  strict_types probes).
- **funcs**: every curated high-risk native function (decoders, matchers,
  formatters, allocators, sorters, callbacks — see `src/funcs.rs`) called with
  ~50 hostile argument shapes: wrong types, boundary ints, NaN/INF,
  self-referential arrays, huge repeat counts, catastrophic regexes, depth
  limits, multi-argument forms.

## Minimising a finding

1. Re-run the saved case 3 times on a clean build; record signal/exit and
   stderr.
2. Shrink by hand (delete lines/statements while the crash persists), or let
   the mutation loop run with the case as the only seed.
3. `git diff`-style note of the smallest reproducer goes into
   `tests/security/` with the command to run it.
