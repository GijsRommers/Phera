--TEST--
static
--FILE--
<?php
function f(){static $n=0;return ++$n;}echo f(),f(),f();
--EXPECT--
123