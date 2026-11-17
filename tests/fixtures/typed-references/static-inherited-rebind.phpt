--TEST--
Typed reference bookkeeping: static-inherited-rebind
--FILE--
<?php
class P { public static int $n = 0;
} class C extends P {}
$old =& P::$n;
$new = 8;
C::$n =& $new;
$old = "free";
try { $new = [];
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
var_dump(P::$n, C::$n, $old);
--EXPECT--
Cannot assign array to reference held by property P::$n of type int
int(8)
int(8)
string(4) "free"
