--TEST--
Class members: object-cast-identity
--FILE--
<?php $a=new stdClass; var_dump($a===(object)$a);
--EXPECT--
bool(true)
