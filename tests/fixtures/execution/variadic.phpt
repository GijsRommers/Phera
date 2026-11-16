--TEST--
variadic
--FILE--
<?php
function f(...$args){foreach($args as $x)echo $x;}f(1,2,3);
--EXPECT--
123