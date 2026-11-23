# Branch directly on case comparisons

The initial condition lowering covers statements and expressions that call
`compile_condition`. The `switch` and `match` emitters build their own
comparisons. Their results are only used to select a case. The later
compare-and-branch pass leaves some of these pairs in place, including in
functions with exception handlers.

Emit the existing comparison branch where each case is compiled. The
branch uses loose equality for `switch` and strict identity for `match`.
It keeps the comparison's source line and uses the same subject and case
operands. Evaluation and cleanup stay at their existing points.

| Structure | Was | Is |
|---|---|---|
| A case test before bytecode optimization | Compare into a temporary, then jump on that temporary | Compare and jump in one instruction |
| A case comparison that calls PHP or warns | General comparison handler at the comparison's source line | Existing comparison branch fallback at the same source line |

A broader late liveness pass would need to prove that the old result slot
owns nothing and that no successor needs the stored result. Case lowering
already knows neither the comparison result nor its slot is needed.
