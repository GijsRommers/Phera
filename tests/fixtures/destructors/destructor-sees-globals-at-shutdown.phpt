--TEST--
destructor-sees-globals-at-shutdown
--FILE--
<?php
class D { function __destruct() { global $log; echo "log has ", count($log), "\n"; } }
$log = [1, 2];
$d = new D;
echo "end\n";
--EXPECT--
end
log has 2
