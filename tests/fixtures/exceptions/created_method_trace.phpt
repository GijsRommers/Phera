--TEST--
Exceptions: created_method_trace
--FILE--
<?php
class Factory {function make($number){return new Exception("x");}} $error=(new Factory)->make(5.0); echo $error->getTraceAsString(); var_dump($error->getTrace()[0]["args"]);
--EXPECTF--
#0 %s(2): Factory->make(5.0)
#1 {main}array(1) {
  [0]=>
  float(5)
}
