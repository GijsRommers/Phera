--TEST--
throw-surfaces-at-release
--FILE--
<?php
class T { function __destruct() { throw new Exception("from destructor"); } }
$t = new T;
try { unset($t); echo "not reached\n"; } catch (Exception $e) { echo "caught ", $e->getMessage(), "\n"; }
echo "end\n";
--EXPECT--
caught from destructor
end
