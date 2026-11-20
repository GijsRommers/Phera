--TEST--
exit-inside-destructor
--FILE--
<?php
class D { function __construct(public $n) {} function __destruct() { echo "~{$this->n}\n"; if ($this->n === "b") exit("exited\n"); } }
$a = new D("a"); $b = new D("b"); $c = new D("c");
echo "end\n";
--EXPECT--
end
~c
~b
exited
