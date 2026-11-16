--TEST--
include-directory5
--FILE--
<?php ini_set('include_path', '.'); @include "absent5.inc"; include '.';
--EXPECTF--

Warning: include(%s): Failed to open stream: No such file or directory in %s on line 1

Warning: include(): Failed opening '.' for inclusion (include_path='.') in %s on line 1
