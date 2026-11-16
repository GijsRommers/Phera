--TEST--
include_once-directory5
--FILE--
<?php ini_set('include_path', '.'); @include "absent5.inc"; include_once '.';
--EXPECTF--

Warning: include_once(%s): Failed to open stream: No such file or directory in %s on line 1

Warning: include_once(): Failed opening '.' for inclusion (include_path='.') in %s on line 1
