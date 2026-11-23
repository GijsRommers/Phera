--TEST--
Shutdown callbacks can pass throwables to the exception handler
--FILE--
<?php
set_exception_handler(fn (Throwable $e) => print "handled " . $e->getMessage());
register_shutdown_function(fn () => throw new Exception('first; '));
register_shutdown_function(fn () => throw new Exception('second; '));
register_shutdown_function(fn () => print 'next');
echo 'script: ';
--EXPECT--
script: handled first; handled second; next
