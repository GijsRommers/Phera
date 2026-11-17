--TEST--
Class members: default-private-integer
--FILE--
<?php class Foo { private $x=42; function read(){var_dump($this->x);} } (new Foo)->read();
--EXPECT--
int(42)
