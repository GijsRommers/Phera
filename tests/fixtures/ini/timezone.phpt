--TEST--
Explicit timezone takes precedence over the configured default
--FILE--
<?php
var_dump(date_default_timezone_get());
ini_set('date.timezone', 'Europe/Brussels');
var_dump(date_default_timezone_get());
date_default_timezone_set('America/New_York');
ini_restore('date.timezone');
var_dump(date_default_timezone_get(), ini_get('date.timezone'));
--EXPECT--
string(3) "UTC"
string(15) "Europe/Brussels"
string(16) "America/New_York"
string(3) "UTC"
