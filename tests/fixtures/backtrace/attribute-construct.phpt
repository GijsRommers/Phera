--TEST--
An attribute constructor points at the declaration and reflection call
--FILE--
<?php
#[Attribute]
class Example {
    function __construct() { debug_print_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS); }
}
#[Example]
function tagged() {}
(new ReflectionFunction('tagged'))->getAttributes()[0]->newInstance();
--EXPECTF--
#0 %s/attribute-construct.php(6): Example->__construct()
#1 %s/attribute-construct.php(8): ReflectionAttribute->newInstance()
