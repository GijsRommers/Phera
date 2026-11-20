--TEST--
break-and-goto-out-of-foreach
--FILE--
<?php
class A { function __construct(public $n) {} function __destruct() { echo "~{$this->n}\n"; } }
foreach ([1, 2] as $i) { foreach ([new A('x'), new A('y')] as $o) { break 2; } }
echo "after break 2\n";
foreach ([new A('g')] as $o) { goto out; }
out: echo "after goto\n";
--EXPECT--
~y
after break 2
~x
after goto
~g
