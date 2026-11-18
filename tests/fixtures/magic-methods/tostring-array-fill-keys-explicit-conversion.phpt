--TEST--
array_fill_keys converts values before inserting the resulting keys
--FILE--
<?php
class Text {
    function __construct(public $text) {}
    function __toString() { echo "[$this->text]"; return $this->text; }
}
var_dump(array_fill_keys([new Text('key')], 3));
--EXPECT--
[key]array(1) {
  ["key"]=>
  int(3)
}
