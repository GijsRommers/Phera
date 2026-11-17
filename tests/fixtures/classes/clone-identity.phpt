--TEST--
Classes: clone-identity
--FILE--
<?php class Box {} $first=new Box; $second=clone $first; var_dump($first === $second, $first == $second);
--EXPECT--
bool(false)
bool(true)
