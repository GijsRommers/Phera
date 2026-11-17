--TEST--
Classes: method-this
--FILE--
<?php class Box { function receiver() { return $this; } } $box=new Box; var_dump($box === $box->receiver());
--EXPECT--
bool(true)
