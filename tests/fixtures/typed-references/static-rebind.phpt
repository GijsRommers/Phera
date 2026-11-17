--TEST--
Typed reference bookkeeping: static-rebind
--FILE--
<?php
class C { public static int $n = 0;
}
$old =& C::$n;
$new = 8;
C::$n =& $new;
$old = "free";
try { $new = [];
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
var_dump($old, C::$n);
--EXPECT--
Cannot assign array to reference held by property C::$n of type int
string(4) "free"
int(8)
