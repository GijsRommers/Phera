--TEST--
strtr converts a replacement object on every match
--FILE--
<?php
class Text {
    function __construct(public $text) {}
    function __toString() { echo "[$this->text]"; return $this->text; }
}
var_dump(strtr('aa', ['a' => new Text('b'), 'x' => new Text('unused')]));
--EXPECT--
[b][b]string(2) "bb"
