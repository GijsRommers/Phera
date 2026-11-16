--TEST--
Class conformance: type-argument-nullable-P
--FILE--
<?php interface I {} class P {} class C extends P implements I {static function hi(){return 1;}} function take(?P $value) {echo "ok";} take(null);
--EXPECT--
ok
