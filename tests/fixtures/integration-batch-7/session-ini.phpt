--TEST--
Session INI functions and registry listings agree on runtime changes
--FILE--
<?php
$initial = ini_get('session.gc_maxlifetime');
$oldLifetime = ini_set('session.gc_maxlifetime', '2k');
$local = ini_get_all('session')['session.gc_maxlifetime'];
$oldPath = ini_set('session.save_path', '/example');
$path = session_save_path();
ini_restore('session.gc_maxlifetime');
var_dump($initial, $oldLifetime, $local, $oldPath, $path, ini_get('session.gc_maxlifetime'));
--EXPECT--
string(4) "1440"
string(4) "1440"
array(3) {
  ["global_value"]=>
  string(4) "1440"
  ["local_value"]=>
  string(2) "2k"
  ["access"]=>
  int(7)
}
string(0) ""
string(8) "/example"
string(4) "1440"
