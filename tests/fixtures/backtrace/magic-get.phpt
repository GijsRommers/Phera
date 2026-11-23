--TEST--
A magic getter appears with its property argument
--FILE--
<?php
class Example {
    function __get($name) { debug_print_backtrace(); }
}
(new Example)->missing;
--EXPECTF--
#0 %s/magic-get.php(5): Example->__get('missing')
