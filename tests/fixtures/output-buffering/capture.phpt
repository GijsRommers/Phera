--TEST--
Every output path goes to the current buffer
--FILE--
<?php
ob_start();
?>inline <?php
echo 'echo ';
print 'print ';
printf('%s ', 'printf');
var_dump(23);
$read = ob_get_contents();
$length = ob_get_length();
$captured = ob_get_clean();
var_dump($read === $captured, $length, ob_get_level(), bin2hex($captured));
--EXPECT--
bool(true)
int(33)
int(0)
string(66) "696e6c696e65206563686f207072696e74207072696e746620696e74283233290a"
