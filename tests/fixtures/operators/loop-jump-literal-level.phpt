--TEST--
A break level must be a positive integer literal
--FILE--
<?php
while (true) {
    $level = 1;
    break $level;
}
--EXPECTF--
Fatal error: 'break' operator with non-integer operand is no longer supported in %s on line 4
