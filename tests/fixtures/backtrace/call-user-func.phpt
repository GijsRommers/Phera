--TEST--
A direct call_user_func is transparent in PHP traces
--FILE--
<?php
function inspect($value) { debug_print_backtrace(); }
call_user_func('inspect', 5);
--EXPECTF--
#0 %s/call-user-func.php(3): inspect(5)
