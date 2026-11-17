--TEST--
Class conformance: type-argument-P-or-null
--FILE--
<?php interface I {} class P {} class C extends P implements I {static function hi(){return 1;}} function take(P|null $value) {echo "ok";} take(new C);
--EXPECT--
ok
