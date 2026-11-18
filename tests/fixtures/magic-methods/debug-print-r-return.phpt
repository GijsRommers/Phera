--TEST--
debug print r return
--FILE--
<?php
class Report {
    public $real = 1;
    function __debugInfo(): array { return ['virtual' => 2, 0 => 3]; }
    static function __set_state(array $props): object { echo "state called"; return new self; }
}
$o = new Report;
echo print_r($o, true);
--EXPECT--
Report Object
(
    [virtual] => 2
    [0] => 3
)
