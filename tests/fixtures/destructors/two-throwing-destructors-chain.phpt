--TEST--
two-throwing-destructors-chain
--FILE--
<?php
class T { function __construct(public $n) {} function __destruct() { throw new Exception("from {$this->n}"); } }
$list = [new T("first"), new T("second")];
try { $list = null; } catch (Exception $e) { echo $e->getMessage(), " <- ", $e->getPrevious()?->getMessage(), "\n"; }
echo "end\n";
--EXPECT--
from second <- from first
end
