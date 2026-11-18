--TEST--
substr_replace casts object strings and replacements at each array position
--FILE--
<?php
class Text {
    function __construct(public $text) {}
    function __toString() { echo "[$this->text]"; return $this->text; }
}
var_dump(substr_replace([new Text('aaa'), new Text('bbb')], [new Text('A'), new Text('B')], 1, 1));
--EXPECT--
[aaa][A][bbb][B]array(2) {
  [0]=>
  string(3) "aAa"
  [1]=>
  string(3) "bBb"
}
