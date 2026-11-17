--TEST--
bind to private
--FILE--
<?php
class Box{private $x=7;}$f=function(){return $this->x;};$g=$f->bindTo(new Box,Box::class);var_dump($g());
--EXPECT--
int(7)
