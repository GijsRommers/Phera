--TEST--
Class conformance: reference-typed-instance-coercion
--FILE--
<?php class Box {public int $value=1;} $box=new Box;$alias=&$box->value;$alias="8";var_dump($box->value);
--EXPECT--
int(8)
