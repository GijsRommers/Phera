--TEST--
Object class uses canonical name
--FILE--
<?php
class Original {}
class_alias(Original::class, "Another");
$object = new Another;
var_dump($object::class);
--EXPECT--
string(8) "Original"
