--TEST--
Loaded extensions has core
--FILE--
<?php
var_dump(in_array("Core",get_loaded_extensions(),true));
--EXPECT--
bool(true)
