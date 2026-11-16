--TEST--
Class conformance: type-property-P
--FILE--
<?php interface I {} class P {} class C extends P implements I {static function hi(){return 1;}} class Box {public P $value;} $box=new Box;$box->value=new C;echo "ok";
--EXPECT--
ok
