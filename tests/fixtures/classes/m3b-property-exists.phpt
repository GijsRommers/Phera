--TEST--
Class members: property-exists
--FILE--
<?php class Foo {private $x;public int $y;} $a=new Foo; var_dump(property_exists($a,"x"),property_exists($a,"y"),property_exists($a,"z"));
--EXPECT--
bool(true)
bool(true)
bool(false)
