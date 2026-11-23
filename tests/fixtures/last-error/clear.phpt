--TEST--
The last error is empty initially and after an explicit clear
--FILE--
<?php
var_dump(error_get_last());
@trigger_error('remember');
var_dump(error_clear_last());
var_dump(error_get_last());
--EXPECT--
NULL
NULL
NULL
