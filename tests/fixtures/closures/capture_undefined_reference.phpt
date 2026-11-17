--TEST--
capture undefined reference
--FILE--
<?php
$f=function()use(&$a){$a=4;};$f();var_dump($a);
--EXPECT--
int(4)
