--TEST--
preg_replace_callback converts the object returned for a match
--FILE--
<?php
class Text {
    function __toString() { echo "cast\n"; return 'abc'; }
}
var_dump(preg_replace_callback('/x/', fn() => new Text, 'x'));
--EXPECT--
cast
string(3) "abc"
