--TEST--
A suppressed engine deprecation stores its exact severity
--FILE--
<?php
@strlen(null);
echo error_get_last()['type'], ":", error_get_last()['message'], "\n";
--EXPECT--
8192:strlen(): Passing null to parameter #1 ($string) of type string is deprecated
