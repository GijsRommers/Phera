--TEST--
require_once-missing5
--FILE--
<?php ini_set('include_path', '.'); require_once 'absent5.inc';
--EXPECTF--

Warning: require_once(absent5.inc): Failed to open stream: No such file or directory in %s on line 1

Fatal error: Uncaught Error: Failed opening required 'absent5.inc' (include_path='.') in %s:1
Stack trace:
#0 {main}
  thrown in %s on line 1
