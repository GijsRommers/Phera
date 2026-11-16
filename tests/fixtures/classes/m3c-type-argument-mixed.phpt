--TEST--
Class conformance: type-argument-mixed
--FILE--
<?php interface I {} class P {} class C extends P implements I {static function hi(){return 1;}} function take(mixed $value) {echo "ok";} take(new C);
--EXPECT--
ok
