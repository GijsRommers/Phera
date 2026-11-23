--TEST--
A handled exception never creates a fatal diagnostic
--FILE--
<?php
set_exception_handler(fn() => null);
register_shutdown_function(function() { var_dump(error_get_last()); });
throw new RuntimeException('handled');
--EXPECT--
NULL
