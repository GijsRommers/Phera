--TEST--
Dynamic reads cast Stringable property names
--FILE--
<?php
class Name { function __toString() { echo "name "; return "x"; } }
$o = (object)["x" => 3];
echo $o->{new Name};
--EXPECT--
name 3
