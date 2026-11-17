--TEST--
Class conformance: type-property-nullable-P
--FILE--
<?php interface I {} class P {} class C extends P implements I {static function hi(){return 1;}} class Box {public ?P $value;} $box=new Box;$box->value=null;echo "ok";
--EXPECT--
ok
