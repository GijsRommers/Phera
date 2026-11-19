--TEST--
An assignment binds to the rightmost operand before it
--FILE--
<?php
$b = 1;
$a = 10;
var_dump($a + $b = 5, $b);
var_dump(!$c = 0, $c);
var_dump(!!!$d = 1, $d);
var_dump($a . $e = 'x', $e);
$f = 3;
var_dump($a * $f += 2, $f);
--EXPECT--
int(15)
int(5)
bool(true)
int(0)
bool(false)
int(1)
string(3) "10x"
string(1) "x"
int(50)
int(5)
