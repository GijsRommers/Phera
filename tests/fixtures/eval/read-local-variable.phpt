--TEST--
Read local variable
--FILE--
<?php
function f() { $x = 7; return eval('return $x;'); } var_dump(f());
--EXPECT--
int(7)
