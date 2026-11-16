--TEST--
Class conformance: explicit-static
--FILE--
<?php class P {static function make(){return new static;}} class D extends P {static function build(){return P::make();}} echo D::build()::class;
--EXPECT--
P
