--TEST--
private-destructor-from-global-scope
--FILE--
<?php
class P { private function __destruct() { echo "never\n"; } }
$p = new P;
try { unset($p); } catch (Error $e) { echo $e->getMessage(), "\n"; }
echo "end\n";
--EXPECT--
Call to private P::__destruct() from global scope
end
