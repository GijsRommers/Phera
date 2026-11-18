--TEST--
Nested eval
--FILE--
<?php
var_dump(eval('return eval("return 3;");'));
--EXPECT--
int(3)
