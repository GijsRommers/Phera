--TEST--
clone captures
--FILE--
<?php
$x=1;$f=function()use(&$x){return ++$x;};$g=clone $f;var_dump($f(),$g(),$x);
--EXPECT--
int(2)
int(3)
int(3)
