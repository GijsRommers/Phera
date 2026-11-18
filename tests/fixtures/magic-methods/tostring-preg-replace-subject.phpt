--TEST--
preg_replace casts an object in its subject array
--FILE--
<?php
class Text {
    function __toString() { echo "cast\n"; return 'abc'; }
}
var_dump(preg_replace('/abc/', 'replacement', [new Text]));
--EXPECT--
cast
array(1) {
  [0]=>
  string(11) "replacement"
}
