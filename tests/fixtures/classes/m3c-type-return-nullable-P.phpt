--TEST--
Class conformance: type-return-nullable-P
--FILE--
<?php interface I {} class P {} class C extends P implements I {static function hi(){return 1;}} function give(): ?P {return null;} give();echo "ok";
--EXPECT--
ok
