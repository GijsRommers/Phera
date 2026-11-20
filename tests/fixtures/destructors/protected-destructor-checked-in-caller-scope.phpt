--TEST--
protected-destructor-checked-in-caller-scope
--FILE--
<?php
class P { protected function __destruct() { echo "ran\n"; } }
class C extends P { static function drop() { $p = new P; } }
C::drop();
$p = new P;
try { $p = null; } catch (Error $e) { echo $e->getMessage(), "\n"; }
echo "end\n";
--EXPECTF--

Fatal error: Uncaught Error: Call to protected P::__destruct() from global scope in %s:4
Stack trace:
#0 {main}
  thrown in %s on line 4
