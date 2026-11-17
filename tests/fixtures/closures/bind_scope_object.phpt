--TEST--
bind scope object
--FILE--
<?php
class Box{private $x=9;}$b=new Box;$f=function(){return $this->x;};$g=$f->bindTo($b,$b);var_dump($g());
--EXPECT--
int(9)
