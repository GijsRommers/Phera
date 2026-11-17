--TEST--
arrow static no this
--FILE--
<?php
class Box{function f(){return static fn()=>isset($this);}}$f=(new Box)->f();var_dump($f());
--EXPECT--
bool(false)
