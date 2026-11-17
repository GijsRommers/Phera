--TEST--
Class members: default-protected-string
--FILE--
<?php class Foo { protected $x='hello'; function read(){var_dump($this->x);} } (new Foo)->read();
--EXPECT--
string(5) "hello"
