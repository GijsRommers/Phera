--TEST--
Class members: default-public-null
--FILE--
<?php class Foo { public $x=null; function read(){var_dump($this->x);} } (new Foo)->read();
--EXPECT--
NULL
