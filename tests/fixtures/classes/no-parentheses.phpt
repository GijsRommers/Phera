--TEST--
Classes: no-parentheses
--FILE--
<?php class Box {} $box = new Box; var_dump(is_object($box));
--EXPECT--
bool(true)
