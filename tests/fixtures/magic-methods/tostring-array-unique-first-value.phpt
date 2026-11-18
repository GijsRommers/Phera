--TEST--
array_unique compares object strings and preserves the first original value
--FILE--
<?php
class Text {
    function __construct(public $text) {}
    function __toString() { echo "[$this->text]"; return $this->text; }
}
$o = new Text('key');
var_dump(array_unique([$o, 'key']) === [$o]);
--EXPECT--
[key]bool(true)
