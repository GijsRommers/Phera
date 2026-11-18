--TEST--
Null extensions retains setting
--FILE--
<?php
spl_autoload_extensions(".test"); var_dump(spl_autoload_extensions(null));
--EXPECT--
string(5) ".test"
