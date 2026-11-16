--TEST--
Class conformance: new-static
--FILE--
<?php class P {static function make(){return new static;}} class D extends P {} echo D::make()::class;
--EXPECT--
D
