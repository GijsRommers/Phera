--TEST--
nodiscard-after-released-operands
--FILE--
<?php
// The #[\NoDiscard] warning reaches calls whose operands are released
// after the call: callbacks, first-class callables, closures.
#[\NoDiscard] function test(): int { return 0; }
class Clazz { #[\NoDiscard] function test(): int { return 0; } }
$closure = #[\NoDiscard] function (): int { return 0; };
call_user_func("test");
$fcc = test(...);
$fcc();
$cls = new Clazz;
call_user_func([$cls, "test"]);
$closure();
(void) $closure();
$kept = $closure();
echo "end\n";
--EXPECTF--

Warning: The return value of function test() should either be used or intentionally ignored by casting it as (void) in %s on line 7

Warning: The return value of function test() should either be used or intentionally ignored by casting it as (void) in %s on line 9

Warning: The return value of method Clazz::test() should either be used or intentionally ignored by casting it as (void) in %s on line 11

Warning: The return value of function %s:6}() should either be used or intentionally ignored by casting it as (void) in %s on line 12
end
