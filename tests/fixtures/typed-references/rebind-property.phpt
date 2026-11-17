--TEST--
Typed reference bookkeeping: rebind-property
--FILE--
<?php
class C { public int $n = 0;
}
$c = new C;
$old =& $c->n;
$new = 7;
$c->n =& $new;
$old = "free";
try { $new = [];
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
var_dump($old, $c->n);
--EXPECT--
Cannot assign array to reference held by property C::$n of type int
string(4) "free"
int(7)
