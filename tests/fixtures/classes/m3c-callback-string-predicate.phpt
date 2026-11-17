--TEST--
Class conformance: callback-string-predicate
--FILE--
<?php class C {function hi(){return 'C';} static function stat(){return 'S';}} $object=new C;$callback='C::stat';var_dump(is_callable($callback));
--EXPECT--
bool(true)
