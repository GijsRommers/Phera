--TEST--
assignment-over-last-reference
--FILE--
<?php
class D { function __construct(public $n) {} function __destruct() { echo "~{$this->n}\n"; } }
$a = new D("first");
$a = new D("second");
echo "after\n";
$a = null;
echo "end\n";
--EXPECT--
~first
after
~second
end
