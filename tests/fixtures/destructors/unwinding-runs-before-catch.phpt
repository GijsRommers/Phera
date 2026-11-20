--TEST--
unwinding-runs-before-catch
--FILE--
<?php
class D { function __construct(public $n) {} function __destruct() { echo "~{$this->n}\n"; } }
function work() { $guard = new D("guard"); throw new Exception("failed"); }
try { work(); } catch (Exception $e) { echo "caught ", $e->getMessage(), "\n"; }
echo "end\n";
--EXPECT--
~guard
caught failed
end
