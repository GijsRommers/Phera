--TEST--
SessionHandler can destroy another session file while open
--FILE--
<?php
ob_start();
session_cache_limiter('');
ini_set('session.gc_probability', 0);
$path = tempnam(sys_get_temp_dir(), 'phera-session-');
unlink($path);
mkdir($path);
session_save_path($path);
$file = $path . "/sess_other";
file_put_contents($file, "a|i:1;");
$handler = new SessionHandler();
session_set_save_handler($handler, false);
session_id('current');
session_start();
var_dump($handler->destroy('other'), file_exists($file));
session_destroy();
foreach (glob($path . '/*') as $entry) unlink($entry);
rmdir($path);
--EXPECT--
bool(true)
bool(false)
