--TEST--
Print a resource argument with its resource id
--FILE--
<?php
function inspect($value) { debug_print_backtrace(); }
inspect(fopen('php://memory', 'r'));
--EXPECTF--
#0 %s/resource-argument.php(3): inspect(Resource id #%d)
