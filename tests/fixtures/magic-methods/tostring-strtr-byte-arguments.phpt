--TEST--
strtr weakly casts Stringable byte mapping arguments
--FILE--
<?php
class Text {
    function __construct(public $text) {}
    function __toString() { echo "[$this->text]"; return $this->text; }
}
var_dump(strtr('abc', new Text('ab'), new Text('xy')));
--EXPECT--
[ab][xy]string(3) "xyc"
