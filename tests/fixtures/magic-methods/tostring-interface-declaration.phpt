--TEST--
An interface declaring __toString also extends Stringable
--FILE--
<?php
interface Label { function __toString(); }
var_dump(class_implements(Label::class));
--EXPECT--
array(1) {
  ["Stringable"]=>
  string(10) "Stringable"
}
