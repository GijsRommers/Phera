--TEST--
A trait alias keeps the alias spelling in its trace frame
--FILE--
<?php
trait Inspector {
    function inspect() { debug_print_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS); }
}
class Example { use Inspector { inspect as renamed; } }
(new Example)->renamed();
--EXPECTF--
#0 %s/trait-alias.php(6): Example->renamed()
