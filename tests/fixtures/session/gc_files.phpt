--TEST--
Session file garbage collection by probability and explicit request
--FILE--
<?php
$dir = tempnam(sys_get_temp_dir(), "phera-session-");
unlink($dir);
mkdir($dir);
session_save_path($dir);
ini_set("session.gc_probability", 1);
ini_set("session.gc_divisor", 1);
ini_set("session.gc_maxlifetime", 60);
file_put_contents($dir . "/sess_old", "count|i:1;");
touch($dir . "/sess_old", time() - 120);
session_id("active");
session_start();
var_dump(file_exists($dir . "/sess_old"), file_exists($dir . "/sess_active"));
file_put_contents($dir . "/sess_old", "count|i:1;");
touch($dir . "/sess_old", time() - 120);
var_dump(session_gc(), file_exists($dir . "/sess_old"));
session_destroy();
rmdir($dir);
?>
--EXPECT--
bool(false)
bool(true)
int(1)
bool(false)
