--TEST--
Typed reference bookkeeping: foreach-reference
--FILE--
<?php
class C { public int $n = 1;
public ?int $m = 2;
}
$c = new C;
foreach ($c as &$r) { $r += 1;
}
try { $r = [];
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
unset($c);
$r = "free";
var_dump($r);
--EXPECT--
Cannot assign array to reference held by property C::$m of type ?int
string(4) "free"
