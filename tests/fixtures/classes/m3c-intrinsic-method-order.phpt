--TEST--
Class conformance: intrinsic-method-order
--FILE--
<?php class P {function inherited(){}} class C extends P {function second(){}function first(){}}var_dump(get_class_methods("C"));
--EXPECT--
array(3) {
  [0]=>
  string(6) "second"
  [1]=>
  string(5) "first"
  [2]=>
  string(9) "inherited"
}
