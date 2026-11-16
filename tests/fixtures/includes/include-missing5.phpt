--TEST--
include-missing5
--FILE--
<?php ini_set('include_path', '.'); include 'absent5.inc';
--EXPECTF--

Warning: include(absent5.inc): Failed to open stream: No such file or directory in %s on line 1

Warning: include(): Failed opening 'absent5.inc' for inclusion (include_path='.') in %s on line 1
