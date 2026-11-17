--TEST--
Class members: default-public-expression
--FILE--
<?php class Foo { public $x=2+3*4; function read(){var_dump($this->x);} } (new Foo)->read();
--EXPECT--
int(14)
