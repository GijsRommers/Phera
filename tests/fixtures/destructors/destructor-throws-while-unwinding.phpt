--TEST--
destructor-throws-while-unwinding
--FILE--
<?php
class T { function __destruct() { throw new Exception("from destructor"); } }
function f() { $t = new T; throw new Exception("first"); }
try { f(); } catch (Exception $e) { echo get_class($e), ": ", $e->getMessage(), " <- ", $e->getPrevious()?->getMessage(), "\n"; }
try { try { f(); } finally { echo "finally\n"; } } catch (Exception $e) { echo $e->getMessage(), " <- ", $e->getPrevious()?->getMessage(), "\n"; }
--EXPECT--
Exception: from destructor <- first
finally
from destructor <- first
