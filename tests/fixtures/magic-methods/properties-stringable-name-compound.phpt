--TEST--
A compound update casts its computed property name just once
--FILE--
<?php
class Key { function __toString() { echo "key\n"; return 'counter'; } }
class Item {
    function __get($name) { echo "get $name\n"; return 4; }
    function __set($name, $value) { echo "set $name=$value\n"; }
}
$o = new Item;
var_dump($o->{new Key} += 3);
--EXPECT--
key
get counter
set counter=7
int(7)
