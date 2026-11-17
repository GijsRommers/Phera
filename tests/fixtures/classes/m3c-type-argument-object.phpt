--TEST--
Class conformance: type-argument-object
--FILE--
<?php interface I {} class P {} class C extends P implements I {static function hi(){return 1;}} function take(object $value) {echo "ok";} take(new C);
--EXPECT--
ok
