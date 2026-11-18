--TEST--
debug var export raw
--FILE--
<?php
class Report {
    public $real = 1;
    function __debugInfo(): array { return ['virtual' => 2, 0 => 3]; }
    static function __set_state(array $props): object { echo "state called"; return new self; }
}
$o = new Report;
var_export($o);
--EXPECT--
\Report::__set_state(array(
   'real' => 1,
))
