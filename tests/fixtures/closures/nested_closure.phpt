--TEST--
nested closure
--FILE--
<?php
$x=5;$f=function()use($x){return function($y)use($x){return $x+$y;};};$g=$f();var_dump($g(4));
--EXPECT--
int(9)
