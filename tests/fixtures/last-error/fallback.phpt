--TEST--
A handler returning false delegates recording to the engine
--FILE--
<?php
set_error_handler(function() { var_dump(error_get_last()); return false; });
@trigger_error('fallback');
echo error_get_last()['message'], "\n";
--EXPECT--
NULL
fallback
