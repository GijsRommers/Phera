--TEST--
callable leading separator
--FILE--
<?php
$f="\\strlen";var_dump($f("hello"));
--EXPECT--
int(5)
