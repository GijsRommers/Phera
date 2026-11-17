--TEST--
call statics shared
--FILE--
<?php
class Box{}$f=function(){static $x=0;return ++$x;};var_dump($f(),$f->call(new Box),$f());
--EXPECT--
int(1)
int(2)
int(3)
