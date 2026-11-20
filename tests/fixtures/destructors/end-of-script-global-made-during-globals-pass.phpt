--TEST--
end-of-script-global-made-during-globals-pass
--FILE--
<?php
class D { function __construct(public $n) {} function __destruct() { echo "~{$this->n}\n"; if ($this->n === "b") { $GLOBALS["late"] = new D("late"); } } }
$a = new D("a");
$b = new D("b");
echo "end\n";
--EXPECT--
end
~b
~a
~late
