--TEST--
A loose comparison resumes at each stringable array element
--FILE--
<?php
class Text {
    function __construct(public $text) {}
    function __toString() { echo "[$this->text]"; return $this->text; }
}
var_dump([[new Text('a'), new Text('b')]] == [['a', 'b']]);
--EXPECT--
[a][b]bool(true)
