--TEST--
Typed reference bookkeeping: failed-binding
--FILE--
<?php
class C { public int $n = 1;
}
$c = new C;
$r =& $c->n;
$bad = [];
try { $c->n =& $bad;
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
try { $r = [];
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
$bad = "free";
var_dump($bad, $c->n);
--EXPECT--
Cannot assign array to property C::$n of type int
Cannot assign array to reference held by property C::$n of type int
string(4) "free"
int(1)
