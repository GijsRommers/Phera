--TEST--
Class conformance: type-return-callable
--FILE--
<?php interface I {} class P {} class C extends P implements I {static function hi(){return 1;}} function give(): callable {return ['C','hi'];} give();echo "ok";
--EXPECT--
ok
