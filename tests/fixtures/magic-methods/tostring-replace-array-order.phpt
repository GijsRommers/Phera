--TEST--
str_replace converts search and replacement elements per subject
--FILE--
<?php
class Text {
    function __construct(public $text) {}
    function __toString() { echo "[$this->text]"; return $this->text; }
}
var_dump(str_replace([new Text('a')], [new Text('b'), new Text('unused')], [new Text('a'), new Text('x')]));
--EXPECT--
[a][a][b][x][a][b]array(2) {
  [0]=>
  string(1) "b"
  [1]=>
  string(1) "x"
}
