--TEST--
Phpversion unknown module
--FILE--
<?php
var_dump(phpversion("phera_no_such_extension"));
--EXPECT--
bool(false)
