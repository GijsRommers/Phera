--TEST--
Typed reference bookkeeping: returned-reference
--FILE--
<?php
class C { public int $n = 1;
}
function &get() { $c = new C;
return $c->n;
}
$r =& get();
$r = "free";
var_dump($r);
--EXPECT--
string(4) "free"
