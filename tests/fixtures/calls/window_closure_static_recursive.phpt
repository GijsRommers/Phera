--TEST--
Call window: closure static recursive
--FILE--
<?php
$f=function($n)use(&$f){static $x=0;$x++;return $n?$f($n-1):$x;};echo $f(3),':',$f(1);
--EXPECT--
4:6
