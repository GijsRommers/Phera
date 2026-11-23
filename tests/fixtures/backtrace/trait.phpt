--TEST--
A trait method frame identifies its using class
--FILE--
<?php
trait Inspector {
    function inspect() { debug_print_backtrace(); }
}
class Example { use Inspector; }
(new Example)->inspect();
--EXPECTF--
#0 %s/trait.php(6): Example->inspect()
