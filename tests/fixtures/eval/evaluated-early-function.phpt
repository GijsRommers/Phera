--TEST--
Evaluated early function
--FILE--
<?php
var_dump(eval('return declared_later(); function declared_later() { return 9; }'));
--EXPECT--
int(9)
