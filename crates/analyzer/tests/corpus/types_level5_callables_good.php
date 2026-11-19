<?php
// Rule: Validated callables carry their parameter types through invocation.
function calledInt(int $value): int { return $value; }
function checkCallables(): void {
    $closed = fn (int $value): int => $value;
    echo $closed(1);
    $firstClass = calledInt(...);
    echo $firstClass(1);
    $name = 'calledInt';
    echo $name(1);
    echo call_user_func('calledInt', 1);
}
