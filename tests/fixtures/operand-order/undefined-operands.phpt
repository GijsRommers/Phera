--TEST--
Binary operand warnings can change another compiled variable
--FILE--
<?php
function rhs() {
    echo "right evaluated\n";
    return 2;
}
set_error_handler(static function ($level, $message) {
    echo $message, "\n";
    $GLOBALS['left'] = 10;
    $GLOBALS['right'] = 20;
    return true;
});
var_dump($left + rhs());
$left = 1;
unset($right);
var_dump($left + $right);
unset($left, $right);
var_dump($left + $right);
--EXPECT--
right evaluated
Undefined variable $left
int(2)
Undefined variable $right
int(10)
Undefined variable $left
int(20)
