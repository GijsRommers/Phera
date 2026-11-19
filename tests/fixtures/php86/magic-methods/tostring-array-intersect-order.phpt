--TEST--
array_intersect casts stringable values in sort and merge order
--FILE--
<?php
class Text {
    function __construct(public $text) {}
    function __toString() { echo "[$this->text]"; return $this->text; }
}
$text = new Text('a');
var_dump(array_intersect([$text, 'k'], ['x', 'a']) === [$text]);
--EXPECT--
[a]bool(true)
