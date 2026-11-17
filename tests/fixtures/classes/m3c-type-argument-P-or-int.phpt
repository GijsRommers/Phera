--TEST--
Class conformance: type-argument-P-or-int
--FILE--
<?php interface I {} class P {} class C extends P implements I {static function hi(){return 1;}} function take(P|int $value) {echo "ok";} take(new C);
--EXPECT--
ok
