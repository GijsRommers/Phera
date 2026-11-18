--TEST--
Default file extensions
--FILE--
<?php
var_dump(spl_autoload_extensions());
--EXPECT--
string(9) ".inc,.php"
