# Retain resolved generic types in conditional returns

- Date: 2026-11-20
- Area: Tools
- Decided by: Design

PHPStan 2.3.7 can turn a resolved generic conditional return into `mixed`
when the experimental `unresolvedTemplateArguments` feature is enabled.
Keep the resolved return in Phera when the declared template and the
argument's type determine it.

Record diagnostics caused by this PHPStan behavior as a separate group
in comparisons. Omit that group from the scored missed-findings rate
and keep the complete configured PHPStan result visible.

The standalone fixture `conditional_resolved_return.php` documents the
inferred type and the PHPStan result with the feature enabled.
