--TEST--
Extension loaded missing
--FILE--
<?php
var_dump(extension_loaded("phera_no_such_extension"));
--EXPECT--
bool(false)
