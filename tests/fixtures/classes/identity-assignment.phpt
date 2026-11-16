--TEST--
Classes: identity-assignment
--FILE--
<?php class Box {} $first=new Box; $second=$first; var_dump($first === $second);
--EXPECT--
bool(true)
