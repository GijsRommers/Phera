--TEST--
Headers are first committed by unbuffered output
--FILE--
<?php
ob_start();
echo 'inside';
$early = headers_sent($nothing, $zero);
echo ob_get_clean();
$sent = headers_sent($file, $line);
var_dump($early, $nothing, $zero, $sent, $file === __FILE__, $line);
--EXPECT--
insidebool(false)
string(0) ""
int(0)
bool(true)
bool(true)
int(5)
