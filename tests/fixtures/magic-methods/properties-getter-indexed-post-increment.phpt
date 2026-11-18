--TEST--
Increment on a by-value getter array operates on the detached value after one notice
--FILE--
<?php
class Items {
    function __get($name) { echo "get $name\n"; return ['k' => 3]; }
}
$o = new Items;
var_dump($o->proxy['k']++, $o->proxy['k']);
--EXPECTF--
get proxy

Notice: Indirect modification of overloaded property Items::$proxy has no effect in %s on line 6
get proxy
int(3)
int(3)
