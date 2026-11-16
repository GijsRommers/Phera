--TEST--
Class conformance: type-return-iterable
--FILE--
<?php interface I {} class P {} class C extends P implements I {static function hi(){return 1;}} function give(): iterable {return [];} give();echo "ok";
--EXPECT--
ok
