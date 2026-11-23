--TEST--
INI file warnings include the scanned path and line
--FILE--
<?php
var_dump(parse_ini_file(__DIR__.'/malformed.inc'));
--EXPECTF--
Warning: syntax error, unexpected end of file, expecting ']' in %s/malformed.inc on line 2
 in %s/file-errors.php on line 2
bool(false)
