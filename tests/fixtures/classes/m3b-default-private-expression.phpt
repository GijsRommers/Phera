--TEST--
Class members: default-private-expression
--FILE--
<?php class Foo { private $x=2+3*4; function read(){var_dump($this->x);} } (new Foo)->read();
--EXPECT--
int(14)
