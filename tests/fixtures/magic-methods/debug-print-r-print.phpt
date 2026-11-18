--TEST--
debug print r print
--FILE--
<?php
class Report {
    public $real = 1;
    function __debugInfo(): array { return ['virtual' => 2, 0 => 3]; }
    static function __set_state(array $props): object { echo "state called"; return new self; }
}
$o = new Report;
print_r($o);
--EXPECT--
Report Object
(
    [virtual] => 2
    [0] => 3
)
