--TEST--
Class conformance: new-parent
--FILE--
<?php class P {} class D extends P {function make(){return new parent;}} echo (new D)->make()::class;
--EXPECT--
P
