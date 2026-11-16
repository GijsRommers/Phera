--TEST--
Class conformance: intrinsic-called-class
--FILE--
<?php class P {static function identify(){return get_called_class();}}class C extends P {}echo C::identify();
--EXPECT--
C
