--TEST--
Invoked objects name the __invoke method
--FILE--
<?php
class Example {
    function __invoke($name) { debug_print_backtrace(); }
}
(new Example)('value');
--EXPECTF--
#0 %s/magic-invoke.php(5): Example->__invoke('value')
