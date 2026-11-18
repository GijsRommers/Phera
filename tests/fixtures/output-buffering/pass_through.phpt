--TEST--
False disables a handler and passes its input through
--FILE--
<?php
ob_start();
ob_start(fn($text, $mode) => false, 2);
echo 'ab';
$disabled = (ob_get_status()['flags'] & PHP_OUTPUT_HANDLER_DISABLED) !== 0;
echo 'cd';
$inner = ob_get_clean();
$outer = ob_get_clean();
var_dump($disabled, $inner, $outer);
--EXPECT--
bool(true)
string(0) ""
string(4) "abcd"
