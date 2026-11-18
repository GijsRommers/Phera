--TEST--
Version compare release candidate
--FILE--
<?php
var_dump(version_compare("8.5.12RC1","8.5.12"));
--EXPECT--
int(-1)
