--TEST--
Call window: argument trace variadic names
--FILE--
<?php
function f($a=1,...$rest){throw new Exception;}try{f(2,3,x:4);}catch(Throwable $e){var_dump($e->getTrace()[0]['args']);}
--EXPECT--
array(3) {
  [0]=>
  int(2)
  [1]=>
  int(3)
  ["x"]=>
  int(4)
}
