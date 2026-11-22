# Use immediate scalar loads after folding literal readers

Date 2026-11-22.

A scalar literal such as the null in an implicit return already fits an instruction. Rewrite the remaining LoadConst instructions for null, booleans, and signed 32-bit integers after the takes pass. This moves their execution to the existing scalar load handlers.

| Structure | Was | Is |
|---|---|---|
| Remaining scalar loads | LoadConst with a constant index, or an immediate load | LoadNull, LoadBool, or LoadInt |
| Assign and send literal folding | Takes pass selects the reader's constant form | Takes pass selects the reader's constant form before the load rewrite |
| Other literals | Indexed in the compiled unit | Indexed in the compiled unit |

Lowering booleans and null directly would hide them from the existing reader folds. Moving LoadConst into the scalar loop regressed the performance gates. The late rewrite keeps instruction positions, source lines, and the VM dispatch unchanged.

The rewrite removed 1,488 table-backed loads from the hello route and 4,056 from the posts route. The request measurements improved by 0.57% and 0.42%. The method microbenchmark measures in discrete bands even with the same executable. Its two candidate batches both exceeded the median limit, so the draft remains gated.
