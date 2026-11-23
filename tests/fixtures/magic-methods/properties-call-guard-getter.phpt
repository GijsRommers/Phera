--TEST--
The getter guard is per object, property and operation
--FILE--
<?php
class Getter {
    function __get($n) { echo "$n\n"; return $n === "x" ? $this->y : $this->y; }
}
var_dump((new Getter)->x);
--EXPECTF--
x
y

Warning: Undefined property: Getter::$y in %s on line 3
NULL
