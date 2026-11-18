--TEST--
debug var dump
--FILE--
<?php
class Report {
    public $real = 1;
    function __debugInfo(): array { return ['virtual' => 2, 0 => 3]; }
    static function __set_state(array $props): object { echo "state called"; return new self; }
}
$o = new Report;
var_dump($o);
--EXPECT--
object(Report)#1 (2) {
  ["virtual"]=>
  int(2)
  [0]=>
  int(3)
}
