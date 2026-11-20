--TEST--
end-of-script-reference-and-static
--FILE--
<?php
class D { function __construct(public $n) {} function __destruct() { echo "~{$this->n}\n"; } }
$r = new D("ref"); $alias = &$r;
function f() { static $s; $s = new D("static"); } f();
class S { static $p; } S::$p = new D("static-property");
$plain = new D("plain");
echo "end\n";
--EXPECT--
end
~plain
~ref
~static
~static-property
