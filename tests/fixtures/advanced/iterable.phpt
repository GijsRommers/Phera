--TEST--
iterable
--FILE--
<?php
function f(iterable $a){foreach($a as $v)echo $v;}f([1,2]);
--EXPECT--
12