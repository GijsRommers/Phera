--TEST--
dynamic_call
--FILE--
<?php
function f($x){echo $x;}$fn='f';$fn(42);
--EXPECT--
42