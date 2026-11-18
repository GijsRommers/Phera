--TEST--
Object equality casts stringable property pairs during its walk
--FILE--
<?php
class Text {
    function __construct(public $text) {}
    function __toString() { echo "[$this->text]"; return $this->text; }
}
var_dump((object) ['p'=>new Text('a')] == (object) ['p'=>'a']);
--EXPECT--
[a]bool(true)
