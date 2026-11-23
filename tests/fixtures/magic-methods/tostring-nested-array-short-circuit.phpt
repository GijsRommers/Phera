--TEST--
A comparison stops casting after the first unequal array entry
--FILE--
<?php
class Text {
    function __construct(public $text) {}
    function __toString() { echo "[$this->text]"; return $this->text; }
}
var_dump([new Text('a'), new Text('unused')] == ['b', 'unused']);
--EXPECT--
[a]bool(false)
