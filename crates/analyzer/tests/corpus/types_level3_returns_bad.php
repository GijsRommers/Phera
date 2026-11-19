<?php
// Rule: return.type reports a known value outside a native or documented contract.
function returnNumber(int|string $input): int {
    if (is_string($input)) {
        return $input;
    }
    return 0;
}
/** @return array{count: int, label?: string} */
function summaryRecord(): array { return ['label' => 'pending']; }
/** @return list<int> */
function numberList(): array { return [1, 'pending']; }
$handler = fn (): int => 'pending';
interface PointLike {}
/** @template T */
class PointResult implements PointLike {}
class OtherResult implements PointLike {}
/** @return PointResult<int> */
function genericResult(OtherResult $value) { return $value; }
