--TEST--
Class members: default-protected-null
--FILE--
<?php class Foo { protected $x=null; function read(){var_dump($this->x);} } (new Foo)->read();
--EXPECT--
NULL
