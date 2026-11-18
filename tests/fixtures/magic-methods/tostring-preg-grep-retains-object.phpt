--TEST--
preg_grep matches the converted string and retains the original value
--FILE--
<?php
class Text {
    function __toString() { echo "cast\n"; return 'abc'; }
}
$text = new Text; var_dump(preg_grep('/abc/', [$text]) === [$text]);
--EXPECT--
cast
bool(true)
