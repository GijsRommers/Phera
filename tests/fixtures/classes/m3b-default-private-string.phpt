--TEST--
Class members: default-private-string
--FILE--
<?php class Foo { private $x='hello'; function read(){var_dump($this->x);} } (new Foo)->read();
--EXPECT--
string(5) "hello"
