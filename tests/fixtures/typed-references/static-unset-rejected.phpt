--TEST--
Typed reference bookkeeping: static-unset-rejected
--FILE--
<?php
class C { public static int $n = 0;
}
$r =& C::$n;
try { unset(C::$n);
} catch (Error $e) { echo $e->getMessage(), "\n";
}
try { $r = [];
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
--EXPECT--
Attempt to unset static property C::$n
Cannot assign array to reference held by property C::$n of type int
