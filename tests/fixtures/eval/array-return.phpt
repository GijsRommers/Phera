--TEST--
Array return
--FILE--
<?php
var_dump(eval('return [1, 2];'));
--EXPECT--
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(2)
}
