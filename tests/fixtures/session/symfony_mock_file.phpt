--TEST--
Symfony-style mock file storage uses ordinary serialization independently
--FILE--
<?php
$dir = tempnam(sys_get_temp_dir(), "phera-mock-session-");
unlink($dir);
mkdir($dir);
$path = $dir . "/fixture.mocksess";
$staged = $path . bin2hex(random_bytes(6));
$bags = ["messages" => ["one", "two"]];
file_put_contents($staged, serialize($bags));
rename($staged, $path);
$saved = unserialize(file_get_contents($path));
unlink($path);
rmdir($dir);
var_dump($saved, session_status(), isset($_SESSION));
?>
--EXPECT--
array(1) {
  ["messages"]=>
  array(2) {
    [0]=>
    string(3) "one"
    [1]=>
    string(3) "two"
  }
}
int(1)
bool(false)
