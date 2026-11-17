--TEST--
Call window: closure captures nested
--FILE--
<?php
$x=3;$f=function($a)use(&$x){$x++;return $a+$x;};function g($a,$b){return $a+$b;}echo g($f(1),$f(2));echo ":$x";
--EXPECT--
12:5
