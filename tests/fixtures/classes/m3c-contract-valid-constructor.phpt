--TEST--
Class conformance: contract-valid-constructor
--FILE--
<?php class P {function __construct(int $value){}} class C extends P {function __construct(){}} echo "ok";
--EXPECT--
ok
