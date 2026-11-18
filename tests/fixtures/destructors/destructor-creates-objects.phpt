--TEST--
destructor-creates-objects
--FILE--
<?php
class D { function __construct(public $n) {} function __destruct() { echo "~{$this->n}\n"; } }
class Maker { function __destruct() { echo "maker\n"; $x = new D("inner"); echo "made\n"; } }
$m = new Maker;
unset($m);
echo "end\n";
--EXPECT--
maker
made
~inner
end
