--TEST--
Regular sorting converts object strings inside array values
--FILE--
<?php
class Text {
    function __construct(public $text) {}
    function __toString() { echo "[$this->text]"; return $this->text; }
}
$text = new Text('a'); $values = [['b'], [$text]]; sort($values); var_dump($values === [[$text], ['b']]);
--EXPECT--
[a]bool(true)
