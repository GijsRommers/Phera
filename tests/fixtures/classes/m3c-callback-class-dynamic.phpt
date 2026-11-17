--TEST--
Class conformance: callback-class-dynamic
--FILE--
<?php class C {function hi(){return 'C';} static function stat(){return 'S';}} $object=new C;$callback=['C','stat'];var_dump($callback());
--EXPECT--
string(1) "S"
