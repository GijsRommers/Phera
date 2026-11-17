--TEST--
Exceptions: trace_mutated_reference
--FILE--
<?php
function create(&$number){$error=new Exception; $number=2;return $error;} $number=1;var_dump(create($number)->getTrace()[0]["args"]);
--EXPECTF--
array(1) {
  [0]=>
  int(1)
}
