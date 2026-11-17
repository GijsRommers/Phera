--TEST--
Class conformance: type-return-mixed
--FILE--
<?php interface I {} class P {} class C extends P implements I {static function hi(){return 1;}} function give(): mixed {return new C;} give();echo "ok";
--EXPECT--
ok
