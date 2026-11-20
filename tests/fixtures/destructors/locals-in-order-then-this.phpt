--TEST--
locals-in-order-then-this
--FILE--
<?php
class D { function __construct(public $n) {} function __destruct() { echo "~{$this->n}\n"; } }
class M { function __destruct() { echo "~this\n"; }
  function run() { $one = new D("one"); $two = new D("two"); unset($GLOBALS["m"]); return 1; } }
$m = new M;
echo $m->run(), "\n";
echo "end\n";
--EXPECT--
~one
~two
~this
1
end
