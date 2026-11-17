--TEST--
Class conformance: callback-class-array
--FILE--
<?php class C {function hi(){return 'C';} static function stat(){return 'S';}} $object=new C;$callback=['C','stat'];var_dump(call_user_func_array($callback,[]));
--EXPECT--
string(1) "S"
