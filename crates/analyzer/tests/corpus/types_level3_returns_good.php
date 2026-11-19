<?php
// Rule: return.type respects native, documented and narrowed value types.
function returnNumber(int|string $input): int {
    if (is_int($input)) {
        return $input;
    }
    return 0;
}
/** @return array{count: int, label?: string} */
function summaryRecord(): array { return ['count' => 2]; }
/** @return list<int> */
function numberList(): array { return array_values(['a' => 2]); }
class ParentResult {
    public function finish(): static { return $this; }
}
class ChildResult extends ParentResult {
    public function finish(): static { return parent::finish(); }
}
$handler = fn (): int => 42;
interface PointLike {}
/** @template T */
class PointResult implements PointLike {}
/** @return PointResult<int> */
function genericResult(PointLike $value) { return $value; }
