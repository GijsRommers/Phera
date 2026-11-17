--TEST--
resolve function string
--FILE--
<?php
function f($x){return $x+1;}$f="f";var_dump($f(2),is_callable($f));
--EXPECT--
int(3)
bool(true)
