<?php
// Rule: Comparison rules distinguish incompatible types and ordered integer intervals.
function checkComparison(int $value): void {
    echo $value === '10';
    echo 12 !== 9;
    echo 2 < 3;
    echo 2 == 3;
}
