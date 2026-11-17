--TEST--
bind default scope
--FILE--
<?php
class Box{private $x=3;function f(){return function(){return $this->x;};}}$f=(new Box)->f();$g=$f->bindTo(new Box);var_dump($f(),$g());
--EXPECT--
int(3)
int(3)
