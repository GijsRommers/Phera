--TEST--
Class conformance: type-argument-P
--FILE--
<?php interface I {} class P {} class C extends P implements I {static function hi(){return 1;}} function take(P $value) {echo "ok";} take(new C);
--EXPECT--
ok
