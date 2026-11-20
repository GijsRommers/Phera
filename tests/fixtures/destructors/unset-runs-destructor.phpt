--TEST--
unset-runs-destructor
--FILE--
<?php
class D { function __construct(public $n) {} function __destruct() { echo "~{$this->n}\n"; } }
$a = new D("a");
echo "before\n";
unset($a);
echo "after\n";
--EXPECT--
before
~a
after
