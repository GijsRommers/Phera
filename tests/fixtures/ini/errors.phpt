--TEST--
INI syntax errors report the scanner line
--FILE--
<?php
var_dump(parse_ini_string("a=1\r\nb=2\n[unfinished"));
--EXPECTF--
Warning: syntax error, unexpected end of file, expecting ']' in Unknown on line 3
 in %s/errors.php on line 2
bool(false)
