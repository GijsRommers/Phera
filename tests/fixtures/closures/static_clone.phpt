--TEST--
static clone
--FILE--
<?php
$f=function(){static $n=0;return ++$n;};$f();$f();$g=clone $f;var_dump($g(),$g(),$f());
--EXPECT--
int(3)
int(4)
int(3)
