--TEST--
static independent
--FILE--
<?php
function make(){return function(){static $n=0;return ++$n;};}$a=make();$b=make();var_dump($a(),$a(),$b(),$a());
--EXPECT--
int(1)
int(2)
int(1)
int(3)
