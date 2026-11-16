--TEST--
Class conformance: callback-object-dynamic
--FILE--
<?php class C {function hi(){return 'C';} static function stat(){return 'S';}} $object=new C;$callback=[$object,'hi'];var_dump($callback());
--EXPECT--
string(1) "C"
