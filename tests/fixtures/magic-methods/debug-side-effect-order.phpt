--TEST--
var_dump emits parent output before nested debug hooks
--FILE--
<?php
class Report { function __debugInfo() { echo "hook\n"; return ["a"=>1]; } }
var_dump([new Report]);
--EXPECT--
array(1) {
  [0]=>
  hook
object(Report)#1 (1) {
    ["a"]=>
    int(1)
  }
}
