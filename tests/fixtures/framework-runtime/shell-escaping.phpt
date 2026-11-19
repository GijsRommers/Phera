--TEST--
Escape shell metacharacters while retaining balanced quotes
--FILE--
<?php
var_dump(escapeshellcmd('echo a;b'), escapeshellcmd("echo 'x' \"x"), escapeshellcmd(""));
try { escapeshellcmd("a\0b"); } catch (ValueError $error) { echo $error->getMessage(), "\n"; }
--EXPECT--
string(9) "echo a\;b"
string(12) "echo 'x' \"x"
string(0) ""
escapeshellcmd(): Argument #1 ($command) must not contain any null bytes
