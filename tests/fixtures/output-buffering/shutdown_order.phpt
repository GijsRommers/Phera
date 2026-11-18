--TEST--
Exit runs shutdown functions before final output handlers
--FILE--
<?php
function decorate($output, $mode) {
    return "$mode:$output";
}
ob_start('decorate');
register_shutdown_function(fn() => print ":shutdown");
register_shutdown_function(fn() => print ":last");
exit('exit');
--EXPECT--
9:exit:shutdown:last
