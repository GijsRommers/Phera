--TEST--
Class conformance: parent-method
--FILE--
<?php class P {function hi(){return 'P';}} class D extends P {function hi(){return parent::hi();}} echo (new D)->hi();
--EXPECT--
P
