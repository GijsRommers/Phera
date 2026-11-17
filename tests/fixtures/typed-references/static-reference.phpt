--TEST--
Typed reference bookkeeping: static-reference
--FILE--
<?php
class C { public static int $n = 0;
}
$r =& C::$n;
$r = 4;
try { $r = [];
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
var_dump(C::$n);
--EXPECT--
Cannot assign array to reference held by property C::$n of type int
int(4)
