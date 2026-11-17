--TEST--
bind scope visibility
--FILE--
<?php
class Box{private static $x=7;}$f=function(){return self::$x;};$g=Closure::bind($f,null,Box::class);var_dump($g());
--EXPECT--
int(7)
