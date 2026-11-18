--TEST--
Unset caller variable
--FILE--
<?php
function f() { $x=7; eval('unset($x);'); return isset($x); } var_dump(f());
--EXPECT--
bool(false)
