--TEST--
A handler that consumes a warning preserves the previous record
--FILE--
<?php
@trigger_error('previous');
set_error_handler(fn() => true);
trigger_error('consumed');
echo error_get_last()['message'], "\n";
--EXPECT--
previous
