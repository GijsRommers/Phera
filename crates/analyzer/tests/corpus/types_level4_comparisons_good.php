<?php
// Rule: Comparison rules distinguish incompatible types and ordered integer intervals.
function checkComparison(int|string $value, int $other): void {
    echo $value === '10';
    echo $other !== 9;
    echo $other < 3;
    echo $other == 3;
}
