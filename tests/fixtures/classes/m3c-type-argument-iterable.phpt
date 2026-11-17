--TEST--
Class conformance: type-argument-iterable
--FILE--
<?php interface I {} class P {} class C extends P implements I {static function hi(){return 1;}} function take(iterable $value) {echo "ok";} take([]);
--EXPECT--
ok
