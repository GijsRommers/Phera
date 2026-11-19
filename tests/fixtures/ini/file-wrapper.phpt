--TEST--
INI files can read a data stream
--FILE--
<?php
var_dump(parse_ini_file('data://text/plain,name=value'));
--EXPECTF--
Warning: parse_ini_file(): data:// wrapper is disabled in the server configuration by allow_url_include=0 in %s/file-wrapper.php on line 2

Warning: parse_ini_file(data://text/plain,name=value): Failed to open stream: no suitable wrapper could be found in %s/file-wrapper.php on line 2
bool(false)
