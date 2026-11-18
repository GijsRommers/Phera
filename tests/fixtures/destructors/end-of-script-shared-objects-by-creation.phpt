--TEST--
end-of-script-shared-objects-by-creation
--FILE--
<?php
class D { function __construct(public $n) {} function __destruct() { echo "~{$this->n}\n"; } }
$a = new D("a"); $a2 = $a;
$b = new D("b");
$c = new D("c"); $c2 = $c;
$list = [new D("in-array")];
echo "end\n";
--EXPECT--
end
~b
~a
~c
~in-array
