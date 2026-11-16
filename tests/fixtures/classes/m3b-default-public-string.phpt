--TEST--
Class members: default-public-string
--FILE--
<?php class Foo { public $x='hello'; function read(){var_dump($this->x);} } (new Foo)->read();
--EXPECT--
string(5) "hello"
