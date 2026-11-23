--TEST--
File sessions, live references, reset, abort, and read-and-close
--FILE--
<?php
ob_start();
$dir = tempnam(sys_get_temp_dir(), "phera-session-");
unlink($dir);
mkdir($dir);
session_save_path($dir);
ini_set("session.gc_probability", "0");
session_id("life");
var_dump(isset($_SESSION), session_start(), session_status());
$_SESSION = ["count" => 1];
$count =& $_SESSION["count"];
$count++;
var_dump(session_encode(), session_commit(), session_status());
var_dump(session_start(), $_SESSION);
$_SESSION["count"] = 99;
var_dump(session_reset(), $_SESSION);
$_SESSION["count"] = 17;
var_dump(session_abort(), $_SESSION);
var_dump(session_start(["read_and_close" => 1]), session_status(), $_SESSION);
var_dump(session_start(), session_unset(), $_SESSION);
var_dump(session_destroy(), session_status(), session_id(), isset($_SESSION));
rmdir($dir);
?>
--EXPECT--
bool(false)
bool(true)
int(2)
string(10) "count|i:2;"
bool(true)
int(1)
bool(true)
array(1) {
  ["count"]=>
  int(2)
}
bool(true)
array(1) {
  ["count"]=>
  int(2)
}
bool(true)
array(1) {
  ["count"]=>
  int(17)
}
bool(true)
int(1)
array(1) {
  ["count"]=>
  int(2)
}
bool(true)
bool(true)
array(0) {
}
bool(true)
int(1)
string(0) ""
bool(true)
