--TEST--
Class members: default-protected-expression
--FILE--
<?php class Foo { protected $x=2+3*4; function read(){var_dump($this->x);} } (new Foo)->read();
--EXPECT--
int(14)
