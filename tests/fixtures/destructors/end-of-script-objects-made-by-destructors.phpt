--TEST--
end-of-script-objects-made-by-destructors
--FILE--
<?php
class D { function __construct(public $n) {} function __destruct() { echo "~{$this->n} ", spl_object_id($this), "\n"; if ($this->n === "x") { $GLOBALS["made"] = new D("made"); $k = new D("tmp"); } } }
$x = new D("x"); $y = $x; $z = new D("z"); $z2 = $z;
echo "end\n";
--EXPECT--
end
~x 1
~tmp 4
~z 2
~made 3
