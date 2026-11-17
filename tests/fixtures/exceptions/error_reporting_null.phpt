--TEST--
Exceptions: error_reporting_null
--FILE--
<?php
$previous=error_reporting();var_dump(error_reporting(null)===$previous);var_dump(error_reporting()===$previous);
--EXPECTF--
bool(true)
bool(true)
