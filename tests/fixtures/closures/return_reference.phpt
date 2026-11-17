--TEST--
return reference
--FILE--
<?php
$a=1;$f=function &()use(&$a){return $a;};$b=&$f();$b=8;var_dump($a);
--EXPECT--
int(8)
