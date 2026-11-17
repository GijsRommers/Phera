--TEST--
callable
--FILE--
<?php
function f(callable $fn){return $fn(3);}function g($a){return $a*2;}echo f('g');
--EXPECT--
6