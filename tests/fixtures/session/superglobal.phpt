--TEST--
Replacing and unsetting the live session variable
--FILE--
<?php
ob_start();
$path = tempnam(sys_get_temp_dir(), "phera-session-");
unlink($path);
mkdir($path);
session_save_path($path);
ini_set("session.gc_probability", 0);
session_id("superglobal");
function replaceSession() {
    $_SESSION = ["original" => 1];
}
session_start();
replaceSession();
$old =& $_SESSION;
unset($_SESSION);
var_dump(isset($_SESSION));
$_SESSION = ["replacement" => 2];
var_dump(session_encode(), $old, $_SESSION);
session_write_close();
session_start();
var_dump($_SESSION, $old);
$_SESSION = "scalar";
var_dump(session_unset(), $_SESSION, @session_encode());
session_destroy();
rmdir($path);
?>
--EXPECT--
bool(false)
string(13) "original|i:1;"
array(1) {
  ["original"]=>
  int(1)
}
array(1) {
  ["replacement"]=>
  int(2)
}
array(1) {
  ["original"]=>
  int(1)
}
array(1) {
  ["original"]=>
  int(1)
}
bool(true)
string(6) "scalar"
bool(false)
