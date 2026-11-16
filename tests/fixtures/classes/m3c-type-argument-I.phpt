--TEST--
Class conformance: type-argument-I
--FILE--
<?php interface I {} class P {} class C extends P implements I {static function hi(){return 1;}} function take(I $value) {echo "ok";} take(new C);
--EXPECT--
ok
