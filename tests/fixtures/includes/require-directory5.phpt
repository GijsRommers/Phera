--TEST--
require-directory5
--FILE--
<?php ini_set('include_path', '.'); @include "absent5.inc"; require '.';
--EXPECTF--

Warning: require(%s): Failed to open stream: No such file or directory in %s on line 1

Fatal error: Uncaught Error: Failed opening required '.' (include_path='.') in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
