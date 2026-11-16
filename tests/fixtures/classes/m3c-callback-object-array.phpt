--TEST--
Class conformance: callback-object-array
--FILE--
<?php class C {function hi(){return 'C';} static function stat(){return 'S';}} $object=new C;$callback=[$object,'hi'];var_dump(call_user_func_array($callback,[]));
--EXPECT--
string(1) "C"
