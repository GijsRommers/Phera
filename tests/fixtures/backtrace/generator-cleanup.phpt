--TEST--
A generator closed during unwinding keeps the failed call site
--FILE--
<?php
function openResource() {
    try { yield 1; }
    finally { debug_print_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS); }
}
class Owner {
    private $resource;
    function __construct() {
        $this->resource = openResource();
        $this->resource->current();
    }
}
Missing::$property = new Owner;
--EXPECTF--
#0 %s/generator-cleanup.php(13): openResource()

Fatal error: Uncaught Error: Class "Missing" not found in %s/generator-cleanup.php:13
Stack trace:
#0 {main}
  thrown in %s/generator-cleanup.php on line 13
