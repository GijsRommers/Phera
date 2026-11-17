--TEST--
Class members: default-protected-integer
--FILE--
<?php class Foo { protected $x=42; function read(){var_dump($this->x);} } (new Foo)->read();
--EXPECT--
int(42)
