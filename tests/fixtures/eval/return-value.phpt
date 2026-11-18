--TEST--
Return value
--FILE--
<?php
var_dump(eval('return 42;'));
--EXPECT--
int(42)
