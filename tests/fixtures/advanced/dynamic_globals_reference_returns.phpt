--TEST--
Dynamic globals, destructuring references and reference returns
--FILE--
<?php
$values = [1, 2];
[&$first, $second] = $values;
$first = 9;
echo $values[0], ':', $second, "\n";
$items = ['k' => 3];
list('k' => &$selected) = $items;
$selected = 8;
echo $items['k'], "\n";
$global = 4;
function change($name) { global $$name; $$name += 5; }
change('global');
echo $global, "\n";
function &counter() { static $value = 10; return $value; }
$alias =& counter();
$alias++;
echo counter(), "\n";
--EXPECT--
9:2
8
9
11
