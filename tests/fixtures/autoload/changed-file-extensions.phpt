--TEST--
Changed file extensions
--FILE--
<?php
var_dump(spl_autoload_extensions(".test,.php"), spl_autoload_extensions());
--EXPECT--
string(10) ".test,.php"
string(10) ".test,.php"
