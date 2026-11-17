--TEST--
bind static private
--FILE--
<?php
class Box{private $x=8;}$f=function(){return $this->x;};$g=Closure::bind($f,new Box,Box::class);var_dump($g());
--EXPECT--
int(8)
