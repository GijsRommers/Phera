--TEST--
Call window: argument trace extras
--FILE--
<?php
function f($a){throw new Exception;}try{f('x',[2],false);}catch(Throwable $e){var_dump($e->getTrace()[0]['args']);}
--EXPECT--
array(3) {
  [0]=>
  string(1) "x"
  [1]=>
  array(1) {
    [0]=>
    int(2)
  }
  [2]=>
  bool(false)
}
