--TEST--
Session serializers, shared aliases, invalid keys, and corrupt data
--FILE--
<?php
ob_start();
$dir = tempnam(sys_get_temp_dir(), "phera-session-");
unlink($dir);
mkdir($dir);
session_save_path($dir);
ini_set("session.gc_probability", "0");
foreach (["php", "php_serialize", "php_binary"] as $format) {
    ini_set("session.serialize_handler", $format);
    session_id("formats");
    session_start();
    $value = 3;
    $_SESSION = ["a" => &$value, "b" => &$value];
    $data = session_encode();
    echo $format, " ", bin2hex($data), "\n";
    $_SESSION = ["old" => 1];
    var_dump(session_decode($data));
    $_SESSION["a"]++;
    var_dump($_SESSION);
    $_SESSION = [5 => "numeric", "bad|key" => 1, str_repeat("k", 128) => 2];
    set_error_handler(function ($type, $message) { echo $message, "\n"; return true; });
    $encoded = session_encode();
    var_dump($encoded === false, is_string($encoded) ? strlen($encoded) : null);
    session_abort();
    if ($format === "php") {
        $bad = 'x|s:99999999999999999999:"x";';
    } elseif ($format === "php_binary") {
        $bad = "\x7Fshort";
    } else {
        $bad = 'a:99999999999999999999:{';
    }
    file_put_contents($dir . "/sess_formats", $bad);
    var_dump(session_start(), session_status(), $_SESSION, file_exists($dir . "/sess_formats"));
    restore_error_handler();
}
rmdir($dir);
?>
--EXPECT--
php 617c693a333b627c523a313b
bool(true)
array(3) {
  ["old"]=>
  int(1)
  ["a"]=>
  &int(4)
  ["b"]=>
  &int(4)
}
session_encode(): Skipping numeric key 5
session_encode(): Failed to write session data. Data contains invalid key "bad|key"
bool(true)
NULL
session_start(): Failed to decode session object. Session has been destroyed
bool(false)
int(1)
array(0) {
}
bool(false)
php_serialize 613a323a7b733a313a2261223b693a333b733a313a2262223b523a323b7d
bool(true)
array(2) {
  ["a"]=>
  &int(4)
  ["b"]=>
  &int(4)
}
bool(false)
int(183)
session_start(): Numerical result out of range
session_start(): Failed to decode session object. Session has been destroyed
bool(false)
int(1)
array(0) {
}
bool(false)
php_binary 0161693a333b0162523a313b
bool(true)
array(3) {
  ["old"]=>
  int(1)
  ["a"]=>
  &int(4)
  ["b"]=>
  &int(4)
}
session_encode(): Skipping numeric key 5
bool(false)
int(12)
session_start(): Failed to decode session object. Session has been destroyed
bool(false)
int(1)
array(0) {
}
bool(false)
