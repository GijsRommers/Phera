--TEST--
Class conformance: callback-string-user
--FILE--
<?php class C {function hi(){return 'C';} static function stat(){return 'S';}} $object=new C;$callback='C::stat';var_dump(call_user_func($callback));
--EXPECT--
string(1) "S"
