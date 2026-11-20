--TEST--
array-elements-in-order
--FILE--
<?php
class D { function __construct(public $n) {} function __destruct() { echo "~{$this->n}\n"; } }
$list = [new D("0"), "k" => new D("k"), new D("2")];
$list = [];
echo "end\n";
--EXPECT--
~0
~k
~2
end
