--TEST--
INI values retain converted bytes and restore their original values
--FILE--
<?php
var_dump(ini_set('user_agent', true), ini_get('user_agent'));
var_dump(ini_alter('user_agent', false), ini_get('user_agent'));
var_dump(ini_set('user_agent', 1.25), ini_set('user_agent', null));
var_dump(ini_restore('user_agent'), ini_get('user_agent'));
var_dump(ini_set('post_max_size', '100M'), ini_get('post_max_size'));
var_dump(ini_set('missing.setting', 'present'), ini_get('missing.setting'));
--EXPECT--
string(0) ""
string(1) "1"
string(1) "1"
string(0) ""
string(0) ""
string(4) "1.25"
NULL
string(0) ""
bool(false)
string(2) "8M"
bool(false)
bool(false)
