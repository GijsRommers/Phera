--TEST--
destructure-nested5
--FILE--
<?php $x=[[1],2]; [[&$a],$b]=$x; $a=7; var_dump($x,$b); $x=['k'=>[3]]; ['k'=>[&$v]]=$x; $v=9; var_dump($x);
--EXPECT--
array(2) {
  [0]=>
  array(1) {
    [0]=>
    &int(7)
  }
  [1]=>
  int(2)
}
int(2)
array(1) {
  ["k"]=>
  array(1) {
    [0]=>
    &int(9)
  }
}
