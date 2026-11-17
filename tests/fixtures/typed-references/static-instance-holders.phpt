--TEST--
Typed reference bookkeeping: static-instance-holders
--FILE--
<?php
class C { public static ?int $s = 0;
public int $n = 1;
}
$c = new C;
$r =& $c->n;
C::$s =& $r;
unset($c);
$r = null;
var_dump(C::$s);
try { $r = [];
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
--EXPECT--
NULL
Cannot assign array to reference held by property C::$s of type ?int
