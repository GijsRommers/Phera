--TEST--
Dynamic property write converts the name after RHS evaluation
--FILE--
<?php
class Name { function __toString() { echo " name "; return "x"; } }
function rhs() { echo "rhs"; return 3; }
$o = new stdClass;
$o->{new Name} = rhs();
echo $o->x;
--EXPECT--
rhs name 3
