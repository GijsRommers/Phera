--TEST--
Unused calls in for clauses warn and an explicit void cast suppresses the warning
--FILE--
<?php
#[NoDiscard]
function f(): int { return 1; }
function consume($value): void {}
set_error_handler(function ($type, $message, $file, $line) {
    echo "$line $message\n";
    return true;
});
f();
(void) f();
for (f(), $i = 0; f(), $i < 1; f(), ++$i) {}
true ? f() : f();
--EXPECT--
9 The return value of function f() should either be used or intentionally ignored by casting it as (void)
11 The return value of function f() should either be used or intentionally ignored by casting it as (void)
11 The return value of function f() should either be used or intentionally ignored by casting it as (void)
11 The return value of function f() should either be used or intentionally ignored by casting it as (void)
11 The return value of function f() should either be used or intentionally ignored by casting it as (void)
