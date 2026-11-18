--TEST--
An unset typed property converts the value returned by a weak getter
--FILE--
<?php
class Text { function __toString() { echo "cast\n"; return 'label'; } }
class Item {
    public string $label = '';
    function __construct() { unset($this->label); }
    function __get($name) { return new Text; }
}
var_dump((new Item)->label);
--EXPECT--
cast
string(5) "label"
