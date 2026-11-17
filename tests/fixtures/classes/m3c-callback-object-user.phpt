--TEST--
Class conformance: callback-object-user
--FILE--
<?php class C {function hi(){return 'C';} static function stat(){return 'S';}} $object=new C;$callback=[$object,'hi'];var_dump(call_user_func($callback));
--EXPECT--
string(1) "C"
