--TEST--
Class conformance: type-argument-P-and-I
--FILE--
<?php interface I {} class P {} class C extends P implements I {static function hi(){return 1;}} function take(P&I $value) {echo "ok";} take(new C);
--EXPECT--
ok
