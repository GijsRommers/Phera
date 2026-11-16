--TEST--
Class conformance: type-argument-callable
--FILE--
<?php interface I {} class P {} class C extends P implements I {static function hi(){return 1;}} function take(callable $value) {echo "ok";} take(['C','hi']);
--EXPECT--
ok
