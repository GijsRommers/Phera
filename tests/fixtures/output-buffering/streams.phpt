--TEST--
Output streams and stream logging write to the current buffer
--FILE--
<?php
ob_start();
$stream = fopen('php://output', 'wb');
$written = fwrite($stream, "a\0b");
fclose($stream);
$whole = file_put_contents('php://output', "c\xff");
$logged = error_log('de', 3, 'php://output');
$captured = ob_get_clean();
var_dump($written, $whole, $logged, bin2hex($captured));
var_dump(error_log('discarded', 3, 'php://input'));
--EXPECT--
int(3)
int(2)
bool(true)
string(14) "61006263ff6465"
bool(false)
