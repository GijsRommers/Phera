# Link a method site to its record and single local

- Date: 2026-11-23
- Area: Speed and method
- Decided by: Design

## Decision

The compiler assigns each method site a record index across the file. Resolving the method checks its signature once. For a plain method with one untyped by-value parameter and a single local send, the existing record holds the local register, function and register count. A hit checks the receiver class index, effective scope, stack limit and current value before it copies the argument into the callee and retains the receiver as `$this`. An undefined local or a reference goes through the original instructions at its send position.

## Why

The call-path ledger attributed 64 excess instructions to the site lookup and 54 to sending the argument. Its estimate for this change was 65–90 retired instructions per method iteration. In the reviewer's settled Symfony profile, the loop entered the method-site arm 40,448 times, costing 23.54M instructions on the profiled revision. That count includes native and general entries.

## Measured result

Drop this implementation. The method shape saved 63.00 instructions per iteration, two below the ledger estimate. All three request routes rose above the 0.10% limit. Every measurement compares the private release executable `review/call-path-astra-item2/parent-c970da78` (the same source tree as main `1fcc6383`) with `review/call-path-astra-item4/phera-item4` at `a2882fca`.

| Shape | Parent instructions / cycles | Candidate instructions / cycles |
| --- | ---: | ---: |
| Loop | 145.192 / 21.807 | 145.190 / 22.896 |
| Function | 679.379 / 94.739 | 679.381 / 96.363 |
| Method | 857.386 / 128.905 | 794.381 / 118.701 |
| Property | 565.194 / 73.922 | 565.191 / 74.610 |

Each column uses the lowest differential per iteration across five shuffled rounds. All checksums matched.

| Route | Parent → candidate, M instructions/request | Repeat, same binaries |
| --- | ---: | ---: |
| Laravel hello | 25.07 → 25.10 (+0.13%) | 25.05 → 25.10 (+0.20%) |
| Laravel posts, page 2 | 106.35 → 106.48 (+0.13%) | 106.37 → 106.50 (+0.13%) |
| Settled Symfony | 440.35 → 441.90 (+0.35%) | Within the pair-spread limit |

Each Laravel run used six alternating pairs. The first hello run crossed the spread limit, so both Laravel routes ran again. Symfony used two pairs with 300 warm-up requests. Both pairs had identical response hashes across builds.

The candidate downcasts the receiver to read its numeric class index before checking the site's record. The release disassembly shows two indirect calls and the type-ID comparison on that path. The parent performs that downcast only on a record miss. Consequently, native and other method calls can pay extra work even when the one-local PHP entry cannot help them. The whole-request counts do not isolate the share attributable to those checks.

The new fixture passed on PHP 8.5.12 and the candidate and its output matched the parent. The ten benchmarks, full fixture suite, VM/runtime tests, destructor and take scripts and both php-src sweeps were beyond the failed request gate.

## Structure: was, is

| Structure | Was | Is |
| --- | --- | --- |
| Site record address | Per-file table of function starts plus function-local site index | File-wide index on the compiled descriptor |
| Class and scope guard | Class-handle address and effective scope index | Receiver class index and effective scope index |
| One-local PHP entry | Walk the argument descriptors and check the parameter on every hit | Record the eligible source and frame width when the site resolves |
| Receiver and fallback | Named receiver retained, taken temporary transferred, ordinary sends available | Same ownership and written-out sends, with specialization limited to retained receivers |

## Turned down

A reference can appear in a local between visits to the site. A rebound closure can also exercise the same site with different private visibility. Both guards must run before the window grows.
