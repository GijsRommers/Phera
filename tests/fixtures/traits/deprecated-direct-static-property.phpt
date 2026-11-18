--TEST--
Direct static trait property access warns on each access
--FILE--
<?php
trait State { public static int $value = 1; }
State::$value = 2;
for ($i = 0; $i < 2; ++$i) { var_dump(State::$value); }
--EXPECTF--
Deprecated: Accessing static trait property State::$value is deprecated, it should only be accessed on a class using the trait in %s on line 3

Deprecated: Accessing static trait property State::$value is deprecated, it should only be accessed on a class using the trait in %s on line 4
int(2)

Deprecated: Accessing static trait property State::$value is deprecated, it should only be accessed on a class using the trait in %s on line 4
int(2)
