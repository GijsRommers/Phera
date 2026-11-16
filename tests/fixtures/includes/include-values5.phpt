--TEST--
include-values5
--FILE--
<?php var_dump(2+(include __DIR__.'/return5.inc')); var_dump(include __DIR__.'/array5.inc'); var_dump(include __DIR__.'/empty5.inc'); var_dump($created); include __DIR__.'/function5.inc'; var_dump(included5());
--EXPECT--
int(7)
array(1) {
  ["k"]=>
  int(8)
}
int(1)
int(12)
int(17)
