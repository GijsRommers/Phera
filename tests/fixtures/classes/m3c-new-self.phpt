--TEST--
Class conformance: new-self
--FILE--
<?php class P {function make(){return new self;}} class D extends P {} echo (new D)->make()::class;
--EXPECT--
P
