--TEST--
Classes: method-this-argument
--FILE--
<?php function identity($value) { return $value; } class Box { function receiver() { return identity($this); } } $box=new Box; var_dump($box === $box->receiver());
--EXPECT--
bool(true)
