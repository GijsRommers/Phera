--TEST--
Class members: default-private-null
--FILE--
<?php class Foo { private $x=null; function read(){var_dump($this->x);} } (new Foo)->read();
--EXPECT--
NULL
