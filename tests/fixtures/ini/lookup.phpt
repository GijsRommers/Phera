--TEST--
Known, missing, and case-sensitive PHP INI names
--FILE--
<?php
var_dump(ini_get('default_charset'), ini_get('memory_limit'), ini_get('error_log'));
var_dump(ini_get('DEFAULT_CHARSET'), ini_get('missing.setting'), ini_get("default_charset\0"));
--EXPECT--
string(5) "UTF-8"
string(4) "128M"
string(0) ""
bool(false)
bool(false)
bool(false)
