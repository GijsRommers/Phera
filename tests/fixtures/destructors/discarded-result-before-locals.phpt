--TEST--
discarded-result-before-locals
--FILE--
<?php
class D { function __construct(public $n) {} function __destruct() { echo "~{$this->n}\n"; } }
function f($p) { $x = new D("local"); return new D("result"); }
f(new D("argument"));
echo "end\n";
--EXPECT--
~result
~argument
~local
end
