--TEST--
Lexical magic constants
--FILE--
<?php
class A { function f() { return eval('return [__CLASS__, __METHOD__, __FUNCTION__, __NAMESPACE__];'); } } var_dump((new A)->f());
--EXPECT--
array(4) {
  [0]=>
  string(0) ""
  [1]=>
  string(0) ""
  [2]=>
  string(0) ""
  [3]=>
  string(0) ""
}
