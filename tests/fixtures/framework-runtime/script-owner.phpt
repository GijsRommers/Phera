--TEST--
Script identity is reported for the entry file
--FILE--
<?php
var_dump(getmyuid() === fileowner(__FILE__), getmygid() === filegroup(__FILE__));
var_dump(getmyinode() === fileinode(__FILE__), is_string(get_current_user()));
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
