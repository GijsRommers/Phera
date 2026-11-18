--TEST--
New local variable
--FILE--
<?php
function f() { eval('$x = 5;'); return $x; } var_dump(f());
--EXPECT--
int(5)
