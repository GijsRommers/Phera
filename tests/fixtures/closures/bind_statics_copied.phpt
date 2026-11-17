--TEST--
bind statics copied
--FILE--
<?php
class Box{}$f=function(){static $x=0;return ++$x;};$f();$g=$f->bindTo(new Box);var_dump($g(),$g(),$f());
--EXPECT--
int(2)
int(3)
int(2)
