--TEST--
first class named statics
--FILE--
<?php
function f(){static $n=0;return ++$n;}$g=f(...);$h=clone $g;var_dump(f(),$g(),$h());
--EXPECT--
int(1)
int(2)
int(3)
