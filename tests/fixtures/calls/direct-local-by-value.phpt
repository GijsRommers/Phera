--TEST--
A direct call reads its local by value and handles an undefined argument before entry
--FILE--
<?php
function change($value, $tail = '!') {
    $value[] = $tail;
    return implode('', $value);
}
function typed(int $value) { return $value + 1; }
function bind(&$value) { $value[] = 'ref'; }
function observe($value) { echo is_null($value) ? "null\n" : "present\n"; }
function run() {
    $array = ['x'];
    echo change($array), ' ', implode('', $array), "\n";
    $alias =& $array;
    echo change($alias), ' ', implode('', $alias), "\n";
    bind($alias);
    echo change($array), ' ', typed('6'), "\n";
    set_error_handler(function ($level, $message, $file, $line) {
        echo "$message on $line\n";
        throw new RuntimeException('read');
    });
    try {
        observe($missing);
    } catch (RuntimeException $e) {
        echo $e->getMessage(), "\n";
    }
    restore_error_handler();
}
run();
--EXPECT--
x! x
x! x
xref! 7
Undefined variable $missing on 21
read
