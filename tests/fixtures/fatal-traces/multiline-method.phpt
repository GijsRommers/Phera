--TEST--
A fatal trace identifies the method name line in a multiline call
--INI--
fatal_error_backtraces=1
--FILE--
<?php
error_reporting(E_ALL & ~E_DEPRECATED);
class TraceSite {
    public function stop(): void {
        trigger_error('test fatal', E_USER_ERROR);
    }
}
(new TraceSite)
    ->stop(
    );
--EXPECTF--

Fatal error: test fatal in %smultiline-method.php on line 5
Stack trace:
#0 %smultiline-method.php(5): trigger_error('test fatal', 256)
#1 %smultiline-method.php(9): TraceSite->stop()
#2 {main}
