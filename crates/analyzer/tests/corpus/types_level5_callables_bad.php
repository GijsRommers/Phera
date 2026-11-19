<?php
// Rule: Validated callables carry their parameter types through invocation.
function calledInt(int $value): int { return $value; }
function checkCallables(): void {
    $closed = fn (int $value): int => $value;
    echo $closed('wrong');
    $firstClass = calledInt(...);
    echo $firstClass('wrong');
    $name = 'calledInt';
    echo $name('wrong');
    echo call_user_func('calledInt', 'wrong');
}
