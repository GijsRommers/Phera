--TEST--
bind static scope
--FILE--
<?php
class Box{private static $x=4;}$f=static function(){return self::$x;};$g=$f->bindTo(null,Box::class);var_dump($g());
--EXPECT--
int(4)
