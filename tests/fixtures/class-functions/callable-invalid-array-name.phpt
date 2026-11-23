--TEST--
Callable invalid array name
--FILE--
<?php
var_dump(is_callable([0, 1], false, $name), $name);
--EXPECT--
bool(false)
string(5) "Array"
