--TEST--
Ternary parentheses, middle operands and short chains group independently
--FILE--
<?php
var_dump((1 ?: 2) ? 3 : 4);
var_dump(0 ?: (2 ? 3 : 4));
var_dump(1 ? 0 ? 2 : 3 : 4);
var_dump(0 ?: 0 ?: 5);
--EXPECT--
int(3)
int(3)
int(3)
int(5)
