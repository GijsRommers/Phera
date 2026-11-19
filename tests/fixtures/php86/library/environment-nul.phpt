--TEST--
NUL bytes in environment lookups and assignments
--FILE--
<?php
putenv('PHERA_FIXTURE_NUL=previous');
try { var_dump(getenv("PHERA_FIXTURE_NUL\0suffix")); }
catch (ValueError $error) { echo $error->getMessage(), PHP_EOL; }
try { var_dump(putenv("PHERA_FIXTURE_NUL=new\0suffix")); }
catch (ValueError $error) { echo $error->getMessage(), PHP_EOL; }
var_dump(getenv('PHERA_FIXTURE_NUL'));
--EXPECT--
getenv(): Argument #1 ($name) must not contain any null bytes
putenv(): Argument #1 ($assignment) must not contain any null bytes
string(8) "previous"
