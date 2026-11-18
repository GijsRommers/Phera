--TEST--
Get defined functions native membership
--FILE--
<?php
var_dump(in_array("strlen",get_defined_functions()["internal"],true));
--EXPECT--
bool(true)
