--TEST--
No PHP configuration file is loaded
--FILE--
<?php
var_dump(get_cfg_var('memory_limit'), php_ini_loaded_file(), php_ini_scanned_files());
--EXPECT--
bool(false)
bool(false)
bool(false)
