--TEST--
Static magic method frames name the handler
--FILE--
<?php
class Example {
    static function __callStatic($name, $arguments) { debug_print_backtrace(); }
}
Example::missing(1);
--EXPECTF--
#0 %s/magic-call-static.php(5): Example::__callStatic('missing', Array)
