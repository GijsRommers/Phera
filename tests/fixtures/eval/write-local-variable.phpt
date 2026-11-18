--TEST--
Write local variable
--FILE--
<?php
function f() { $x = 7; eval('$x += 2;'); return $x; } var_dump(f());
--EXPECT--
int(9)
