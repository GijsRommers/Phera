--TEST--
static method closure
--FILE--
<?php
class Box{private $x=8;function f(){return static function(){return isset($this);};}}$f=(new Box)->f();var_dump($f());
--EXPECT--
bool(false)
