--TEST--
invoke callback map
--FILE--
<?php
class Double{function __invoke($a){return $a*2;}}var_dump(array_map(new Double,[1,2,3]));
--EXPECT--
array(3) {
  [0]=>
  int(2)
  [1]=>
  int(4)
  [2]=>
  int(6)
}
