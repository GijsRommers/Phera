--TEST--
Read dynamic caller variable
--FILE--
<?php
function f() { $name='dynamic'; $$name=12; return eval('return $dynamic;'); } var_dump(f());
--EXPECT--
int(12)
