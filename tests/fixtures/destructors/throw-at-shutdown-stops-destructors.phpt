--TEST--
throw-at-shutdown-stops-destructors
--FILE--
<?php
class D { function __construct(public $n) {} function __destruct() { echo "~{$this->n}\n"; if ($this->n === "thrower") throw new Exception("at shutdown"); } }
$x = new D("x"); $y = $x; $t = new D("thrower"); $t2 = $t; $z = new D("z"); $z2 = $z;
echo "end\n";
--EXPECTF--
end
~x
~thrower

Fatal error: Uncaught Exception: at shutdown in %s:2
Stack trace:
#0 [internal function]: D->__destruct()
#1 {main}
  thrown in %s on line 2
