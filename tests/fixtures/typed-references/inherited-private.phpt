--TEST--
Typed reference bookkeeping: inherited-private
--FILE--
<?php
class P { private int $n = 1;
public function &get() { return $this->n;
} }
class C extends P { public string $n = "child";
}
$c = new C;
$r =& $c->get();
try { $r = [];
} catch (TypeError $e) { echo $e->getMessage(), "\n";
}
var_dump($c->n);
unset($c);
$r = "free";
var_dump($r);
--EXPECT--
Cannot assign array to reference held by property P::$n of type int
string(5) "child"
string(4) "free"
