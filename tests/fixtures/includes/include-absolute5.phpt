--TEST--
include-absolute5
--FILE--
<?php var_dump(include __DIR__.'/return5.inc');
--EXPECTF--
int(5)
