--TEST--
Typed reference bookkeeping: copied-object-array
--FILE--
<?php
class C { public int $n = 0;
}
$a = [new C];
$r =& $a[0]->n;
$copy = $a;
unset($a);
try { $r = [];
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
var_dump($copy[0]->n);
unset($copy);
$r = "free";
var_dump($r);
--EXPECT--
Cannot assign array to reference held by property C::$n of type int
int(0)
string(4) "free"
