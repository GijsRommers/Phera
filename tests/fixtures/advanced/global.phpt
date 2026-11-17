--TEST--
global
--FILE--
<?php
$x=1;function f(){global $x;$x+=2;}f();echo $x;
--EXPECT--
3