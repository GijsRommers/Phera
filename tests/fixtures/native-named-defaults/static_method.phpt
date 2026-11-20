--TEST--
Native named defaults: static method
--FILE--
<?php
$all = DateTimeZone::listIdentifiers();
var_dump(DateTimeZone::listIdentifiers(countryCode: null) === $all);
?>
--EXPECT--
bool(true)
