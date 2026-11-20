--TEST--
shared-object-waits-for-last-reference
--FILE--
<?php
class D { function __construct(public $n) {} function __destruct() { echo "~{$this->n}\n"; } }
$a = new D("shared");
$b = $a;
unset($a);
echo "still held\n";
unset($b);
echo "end\n";
--EXPECT--
still held
~shared
end
