--TEST--
inherited-and-parent-destructor
--FILE--
<?php
class Base { function __destruct() { echo "base\n"; } }
class Child extends Base { function __destruct() { echo "child\n"; parent::__destruct(); } }
class Plain extends Base {}
$c = new Child; unset($c);
$p = new Plain; unset($p);
echo "end\n";
--EXPECT--
child
base
base
end
