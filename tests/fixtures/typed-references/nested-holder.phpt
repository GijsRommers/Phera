--TEST--
Typed reference bookkeeping: nested-holder
--FILE--
<?php
class C { public int $n = 1;
}
$outer = new stdClass;
$outer->items = [new C];
$r =& $outer->items[0]->n;
try { $r = [];
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
unset($outer->items);
$r = "free";
var_dump($r);
--EXPECT--
Cannot assign array to reference held by property C::$n of type int
string(4) "free"
