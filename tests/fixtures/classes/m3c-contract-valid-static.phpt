--TEST--
Class conformance: contract-valid-static
--FILE--
<?php class P {function test(): self{return $this;}} class C extends P {function test(): static{return $this;}}echo "ok";
--EXPECT--
ok
