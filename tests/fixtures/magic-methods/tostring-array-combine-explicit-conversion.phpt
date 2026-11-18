--TEST--
array_combine converts values to strings before using them as keys
--FILE--
<?php
class Text {
    function __construct(public $text) {}
    function __toString() { echo "[$this->text]"; return $this->text; }
}
var_dump(array_combine([new Text('key')], [3]));
--EXPECT--
[key]array(1) {
  ["key"]=>
  int(3)
}
