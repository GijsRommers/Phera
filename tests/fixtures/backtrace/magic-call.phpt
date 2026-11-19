--TEST--
Magic method frames name the handler
--FILE--
<?php
class Example {
    function __call($name, $arguments) { debug_print_backtrace(); }
}
(new Example)->missing(flag: false);
--EXPECTF--
#0 %s/magic-call.php(5): Example->__call('missing', Array)
