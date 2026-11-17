--TEST--
Constant expressions in defaults and static initializers
--FILE--
<?php
const BASE = 6;
function defaults($first = BASE * 2, array $items = ['k' => BASE + 1]) { echo $first, ':', $items['k'], "\n"; }
defaults();
defaults(items: ['k' => 9]);
function counter() { static $value = BASE + 2; return ++$value; }
echo counter(), ':', counter(), "\n";
--EXPECT--
12:7
12:9
9:10
