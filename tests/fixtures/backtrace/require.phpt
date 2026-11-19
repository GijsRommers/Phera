--TEST--
Require displays its own name and file argument
--FILE--
<?php
function inspect() { debug_print_backtrace(); }
require __DIR__.'/trace-file.inc';
--EXPECTF--
#0 %s/trace-file.inc(2): inspect()
#1 %s/require.php(3): require('/private/var/fo...')
