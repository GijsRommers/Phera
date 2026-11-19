--TEST--
A lazy ghost can start a session and read a session directive
--FILE--
<?php
class Payload { public string $value; }
$dir = sys_get_temp_dir().'/phera-lazy-session-'.getmypid();
mkdir($dir);
session_save_path($dir);
ini_set('session.gc_probability', '0');
session_id('lazy');
$ghost = (new ReflectionClass(Payload::class))->newLazyGhost(function (Payload $object) {
    session_start();
    $object->value = ini_get('session.gc_maxlifetime').':'.session_status();
    session_destroy();
});
$value = $ghost->value;
rmdir($dir);
var_dump($value, session_status());
--EXPECT--
string(6) "1440:2"
int(1)
