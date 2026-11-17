--TEST--
Class conformance: type-return-P-or-null
--FILE--
<?php interface I {} class P {} class C extends P implements I {static function hi(){return 1;}} function give(): P|null {return new C;} give();echo "ok";
--EXPECT--
ok
