--TEST--
include-scope5
--FILE--
<?php $caller=1; var_dump(include __DIR__.'/scope5.inc'); var_dump($caller,$created); function run5() { $caller=10; var_dump(include __DIR__.'/scope5.inc'); var_dump($caller,$created); } run5();
--EXPECT--
array(2) {
  [0]=>
  int(3)
  [1]=>
  int(9)
}
int(4)
int(9)
array(2) {
  [0]=>
  int(12)
  [1]=>
  int(9)
}
int(13)
int(9)
