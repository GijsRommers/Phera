--TEST--
Shutdown callbacks run in registration order after exit
--FILE--
<?php
register_shutdown_function(function ($label) {
    echo $label, "\n";
    register_shutdown_function(fn() => print "last\n");
}, "first");
register_shutdown_function(fn() => print "second\n");
header_register_callback(fn() => print "headers\n");
echo "main\n";
exit(7);
--EXPECT--
headers
main
first
second
last
