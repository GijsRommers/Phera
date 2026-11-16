--TEST--
Class conformance: callback-object-predicate
--FILE--
<?php class C {function hi(){return 'C';} static function stat(){return 'S';}} $object=new C;$callback=[$object,'hi'];var_dump(is_callable($callback));
--EXPECT--
bool(true)
