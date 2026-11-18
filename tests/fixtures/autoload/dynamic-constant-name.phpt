--TEST--
Dynamic constant name
--FILE--
<?php
spl_autoload_register(function($name) { echo $name, "\n"; class A { const X=3; } });
$name = "X";
var_dump(A::{$name});
--EXPECT--
A
int(3)
