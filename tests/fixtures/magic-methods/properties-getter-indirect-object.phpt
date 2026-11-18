--TEST--
An indirect write can mutate an object returned by a getter
--FILE--
<?php
class Getter {
    private $item;
    function __construct() { $this->item = (object)['key' => 2]; }
    function __get($p) { return $this->item; }
}
$o = new Getter;
$o->x->key = 7;
echo $o->x->key;
--EXPECT--
7
