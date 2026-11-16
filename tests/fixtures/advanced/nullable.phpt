--TEST--
nullable
--FILE--
<?php
function f(?int $n):?int{return $n;}echo f(null)??42;
--EXPECT--
42