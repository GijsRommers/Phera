--TEST--
clone static external reference
--FILE--
<?php
$f=function &(){static $n=0;return $n;};$x=&$f();$g=clone $f;$y=&$g();$x=3;var_dump($y);
--EXPECT--
int(3)
