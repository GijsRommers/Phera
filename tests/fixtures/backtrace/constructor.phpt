--TEST--
Constructors appear as object call frames
--FILE--
<?php
class Example {
    function __construct($value) { debug_print_backtrace(); }
}
new Example(123);
--EXPECTF--
#0 %s/constructor.php(5): Example->__construct(123)
