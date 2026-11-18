--TEST--
Typed getter conversion writes the cast string through its reference
--FILE--
<?php
class Text { function __toString() { return 'label'; } }
class Item {
    public string $label = '';
    public $storage;
    function __construct() { $this->storage = new Text; unset($this->label); }
    function &__get($name) { return $this->storage; }
}
$o = new Item;
var_dump($o->label, $o->storage);
--EXPECT--
string(5) "label"
string(5) "label"
