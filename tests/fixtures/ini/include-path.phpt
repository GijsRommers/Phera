--TEST--
Include-path functions use the same setting and reject an empty path
--FILE--
<?php
set_include_path('lib');
var_dump(get_include_path(), ini_get('include_path'));
var_dump(ini_set('include_path', 'src'), get_include_path(), set_include_path(''));
--EXPECT--
string(3) "lib"
string(3) "lib"
string(3) "lib"
string(3) "src"
bool(false)
