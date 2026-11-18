--TEST--
A nested increment retains the receiver returned by __get
--FILE--
<?php
class Child {
    function __get($name) { echo "child $name\n"; return 4; }
    function __set($name, $value) { echo "$name=$value\n"; }
}
class ParentValue { function __get($name) { echo "parent $name\n"; return new Child; } }
$o = new ParentValue;
var_dump($o->child->counter++);
--EXPECT--
parent child
child counter
counter=5
int(4)
