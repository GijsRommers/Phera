--TEST--
Class members: default-public-integer
--FILE--
<?php class Foo { public $x=42; function read(){var_dump($this->x);} } (new Foo)->read();
--EXPECT--
int(42)
