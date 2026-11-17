--TEST--
Call window: argument trace reassigned
--FILE--
<?php
function f($a){$a=7;throw new Exception;}try{f(2,'extra');}catch(Throwable $e){var_dump($e->getTrace()[0]['args']);}
--EXPECT--
array(2) {
  [0]=>
  int(7)
  [1]=>
  string(5) "extra"
}
