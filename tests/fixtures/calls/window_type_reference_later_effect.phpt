--TEST--
Call window: type reference later effect
--FILE--
<?php
function f(float &$a,$b){var_dump($a,$b);} function side(&$x){var_dump($x);return $x;} $x=3;f($x,side($x));var_dump($x);
--EXPECT--
int(3)
float(3)
int(3)
float(3)
