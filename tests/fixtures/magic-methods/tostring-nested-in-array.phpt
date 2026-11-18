--TEST--
in_array converts object strings inside compared arrays without replacing elements
--FILE--
<?php
class Text {
    function __construct(public $text) {}
    function __toString() { echo "[$this->text]"; return $this->text; }
}
var_dump(in_array([new Text('a')], [['a']]));
--EXPECT--
[a]bool(true)
