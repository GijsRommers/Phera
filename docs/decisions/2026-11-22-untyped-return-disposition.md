# Record an untyped by-value return on a PHP call frame

- Date: 2026-11-22
- Area: Speed and method
- Decided by: Design

## Decision

The function signature records whether its return is untyped, by value, and outside a generator. On entry from a direct or prepared PHP call, an ordinary function or method carries that fact in a spare bit of its 16-byte frame.

At return, the active position supplies the register base. The frame on the stack supplies the current flags, since an instruction can acquire a side-table entry while the loop holds a copy of the frame. A callback boundary, frame extras, or an outstanding argument preparation selects the existing return checks. A reference or undefined source stays in place for the VM. Both paths use the same release and caller-store code.

## Why

The instruction ledger on Apple silicon attributed 59 instructions per ordinary return to finding the frame, verifying the return contract, and checking the source. It counted another 20 to take the value from a slot already addressed. The predicted net saving was 35–55 instructions per function or method iteration.

## Measurement

The entries give retired instructions and elapsed cycles per iteration, each the minimum of five shuffled rounds of the ledger's 1M-to-51M differential. All checksums matched. Each column uses a separate private release executable in `review/call-path-astra-item2/`. The main control was built from a clean detached `3ba1578a` checkout. The parent control includes the borrowed instruction from #200. Comparing the candidate with that parent isolates the effect of this change.

| Shape | `main-3ba1578a` instructions / cycles | `parent-c970da78` instructions / cycles | `phera-item2` at `69f11ae7` instructions / cycles | Instructions saved over parent |
| --- | ---: | ---: | ---: | ---: |
| Loop | 146.27 / 21.30 | 145.28 / 21.90 | 143.28 / 22.04 | 2.00 |
| Function | 713.62 / 101.75 | 679.59 / 94.74 | 644.60 / 86.16 | 34.99 |
| Method | 892.60 / 140.96 | 857.59 / 128.18 | 826.59 / 130.75 | 31.00 |
| Property | 581.30 / 92.08 | 565.31 / 73.83 | 564.29 / 73.79 | 1.02 |

At whole-instruction precision the function reached the lower estimate. The method fell four instructions short. All four shapes retired fewer instructions, so the reviewer advanced this candidate to the three request routes. The focused fixture matched PHP 8.5.12 and both controls.

## Request gate

Drop this candidate. The parent of this item is #200. The posts regression repeated, and settled Symfony also rose. The named pre-#200 main control measures both items together.

| Route | Private `main-3ba1578a` → `phera-item2`, M instructions/request | Private `parent-c970da78` → `phera-item2`, M instructions/request |
| --- | ---: | ---: |
| Laravel hello | 25.50 → 25.20 (−1.18%) | 25.10 → 25.20 (+0.40%), repeat 25.08 → 25.10 (+0.07%) |
| Laravel posts, page 2 | 108.07 → 106.87 (−1.11%) | 106.48 → 106.90 (+0.39%), repeat 106.40 → 106.80 (+0.38%) |
| Settled Symfony | 445.27 → 443.08 (−0.49%) | 440.89 → 442.58 (+0.38%) |

Every Laravel comparison used six alternating pairs. The parent comparison was repeated because the first pairs varied by more than 0.3 percentage points. Each Symfony comparison used two pairs after 300 warm-up requests, with matching response hashes.

A typed return, a frame with extras, or a call returning while an outer send is pending now checks eligibility for the untyped path and then calls `leave_checked`. The parent inlined that handler. In the candidate's ordinary loop, AArch64 disassembly shows a branch-with-link into `leave_checked` from both return arms. The checked path reloads the frame and checks its contract. Method entry also loads the eligibility byte and packs it into the frame flags. These are identifiable extra costs outside the simple shape. The route measurement counts whole requests and cannot apportion the increase among those paths.

The new fixture passed on PHP 8.5.12 and the candidate. Its output also matched both private controls. The route failure stopped the trial before the ten benchmarks, full fixture suite, VM/runtime tests, destructor and take scripts, and both php-src sweeps.

## Structure: was, is

| Structure | Was | Is |
| --- | --- | --- |
| Function signature | A type mask supplied the eventual return-type check | The signature also records the untyped by-value contract |
| PHP frame | A return reloaded the callee's facts | Eligible entries carry the contract in one flag bit |
| Source operand | The return checked a borrowed slot, then indexed it again to take the value | The untyped path checks and takes through one mutable slot |
| Release and result store | Shared by the function and method return kinds | Shared by both the checked and untyped paths |

## Turned down

Treating the flags in the active position as current would miss extras created since frame entry. Assuming a call always returns to PHP would also miss native stop boundaries. Both conditions are checked at the return.
