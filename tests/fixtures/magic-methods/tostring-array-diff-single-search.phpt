--TEST--
array_diff casts candidates until a match is found
--FILE--
<?php
class Text {
    function __construct(public $text) {}
    function __toString() { echo "[$this->text]"; return $this->text; }
}
var_dump(array_diff([new Text('a')], [new Text('b'), new Text('a'), new Text('unused')]));
--EXPECT--
[a][b][a]array(0) {
}
