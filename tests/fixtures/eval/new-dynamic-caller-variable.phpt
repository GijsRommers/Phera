--TEST--
New dynamic caller variable
--FILE--
<?php
function f() { eval('$a = 5;'); $name='a'; return $$name; } var_dump(f());
--EXPECT--
int(5)
