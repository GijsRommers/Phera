--TEST--
closure-capture-released-with-closure
--FILE--
<?php
class D { function __construct(public $n) {} function __destruct() { echo "~{$this->n}\n"; } }
$o = new D("captured");
$f = function () use ($o) { return 1; };
unset($o);
echo "closure holds it\n";
unset($f);
echo "end\n";
--EXPECT--
closure holds it
~captured
end
