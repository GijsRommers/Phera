--TEST--
explicit-call-does-not-mark-destructed
--FILE--
<?php
class D { function __construct(public $n) {} function __destruct() { echo "~{$this->n}\n"; } }
$a = new D("a");
$a->__destruct();
echo "explicit\n";
unset($a);
echo "end\n";
--EXPECT--
~a
explicit
~a
end
