--TEST--
returned-local-in-order
--FILE--
<?php
class D { function __construct(public $n) {} function __destruct() { echo "~{$this->n}\n"; } }
function f() { $a = new D("first"); $v = new D("returned"); return $v; }
f();
echo "end\n";
--EXPECT--
~first
~returned
end
