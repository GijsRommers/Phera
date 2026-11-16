--TEST--
Class conformance: type-return-P-and-I
--FILE--
<?php interface I {} class P {} class C extends P implements I {static function hi(){return 1;}} function give(): P&I {return new C;} give();echo "ok";
--EXPECT--
ok
