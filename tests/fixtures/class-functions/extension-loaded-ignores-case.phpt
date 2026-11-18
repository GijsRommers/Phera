--TEST--
Extension loaded ignores case
--FILE--
<?php
var_dump(extension_loaded("sTaNdArD"));
--EXPECT--
bool(true)
