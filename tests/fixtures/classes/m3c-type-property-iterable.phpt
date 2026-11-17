--TEST--
Class conformance: type-property-iterable
--FILE--
<?php interface I {} class P {} class C extends P implements I {static function hi(){return 1;}} class Box {public iterable $value;} $box=new Box;$box->value=[];echo "ok";
--EXPECT--
ok
