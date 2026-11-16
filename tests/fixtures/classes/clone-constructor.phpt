--TEST--
Classes: clone-constructor
--FILE--
<?php class Box { function __construct() { echo "new"; } } $box=new Box; $copy=clone $box; var_dump($box == $copy);
--EXPECT--
newbool(true)
