--TEST--
private-destructor-at-shutdown
--FILE--
<?php
class P { private function __destruct() { echo "never\n"; } }
$p = new P;
echo "end\n";
--EXPECT--
end

Warning: Call to private P::__destruct() from global scope during shutdown ignored in Unknown on line 0
