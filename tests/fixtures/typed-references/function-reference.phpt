--TEST--
Typed reference bookkeeping: function-reference
--FILE--
<?php
class C { public int $n = 0;
}
function write(&$r, $value) { $r = $value;
}
$c = new C;
write($c->n, 4);
try { write($c->n, []);
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
var_dump($c->n);
--EXPECT--
Cannot assign array to reference held by property C::$n of type int
int(4)
