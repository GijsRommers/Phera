--TEST--
Binary operations consume compiled variables after both expressions
--FILE--
<?php
$global = 1;
function changeGlobal() {
    global $global;
    $global = 5;
    return 3;
}
function readValue($value) {
    return $value;
}
function operands() {
    global $global;
    static $n = 2;
    $a = 1;
    var_dump($a + ($a = 5), $global + changeGlobal(), $n + $n++);
    $a = 2;
    $alias =& $a;
    var_dump($a + ++$alias);
    $array = [1];
    var_dump($array[0] + ($array[0] = 5));
    $a = 1;
    var_dump(readValue($a) + ($a = 5));
    $a = 1;
    var_dump($a += ($a = 5));
}
operands();
$holder = new class {
    public $p = 1;
    public function value() {
        return $this->p + ($this->p = 5);
    }
};
var_dump($holder->value());
--EXPECT--
int(10)
int(8)
int(5)
int(6)
int(6)
int(6)
int(10)
int(6)
