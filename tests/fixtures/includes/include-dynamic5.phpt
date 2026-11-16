--TEST--
include-dynamic5
--FILE--
<?php $hidden=3; var_dump(include __DIR__.'/dynamic5.inc'); var_dump(compact('only_dynamic5')); function dynamic5() { $hidden=9; var_dump(include __DIR__.'/dynamic5.inc'); var_dump(compact('only_dynamic5')); } dynamic5();
--EXPECTF--
array(1) {
  ["hidden"]=>
  int(4)
}
array(1) {
  ["only_dynamic5"]=>
  int(14)
}
array(1) {
  ["hidden"]=>
  int(10)
}
array(1) {
  ["only_dynamic5"]=>
  int(14)
}
