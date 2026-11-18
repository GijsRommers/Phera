--TEST--
A shallow clone of a self-referential object shares its self property
--FILE--
<?php
$o = new stdClass;
$o->self = $o;
var_dump($o == clone $o);
--EXPECT--
bool(true)
