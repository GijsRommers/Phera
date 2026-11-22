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

## Structure: was, is

| Structure | Was | Is |
| --- | --- | --- |
| Function signature | A type mask supplied the eventual return-type check | The signature also records the untyped by-value contract |
| PHP frame | A return reloaded the callee's facts | Eligible entries carry the contract in one flag bit |
| Source operand | The return checked a borrowed slot, then indexed it again to take the value | The untyped path checks and takes through one mutable slot |
| Release and result store | Shared by the function and method return kinds | Shared by both the checked and untyped paths |

## Turned down

Treating the flags in the active position as current would miss extras created since frame entry. Assuming a call always returns to PHP would also miss native stop boundaries. Both conditions are checked at the return.
