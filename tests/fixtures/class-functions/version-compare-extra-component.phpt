--TEST--
Version compare extra component
--FILE--
<?php
var_dump(version_compare("8.5","8.5.0"));
--EXPECT--
int(-1)
