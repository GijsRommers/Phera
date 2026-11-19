--TEST--
PHP 8.5 names closures after their enclosing method
--FILE--
<?php
class Example {
    function inspect() {
        $callback = function() { debug_print_backtrace(); };
        $callback();
    }
}
(new Example)->inspect();
--EXPECTF--
#0 %s/closure.php(5): Example->{closure:Example::inspect():4}()
#1 %s/closure.php(8): Example->inspect()
