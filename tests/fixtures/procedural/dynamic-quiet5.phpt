--TEST--
dynamic-quiet5
--FILE--
<?php $name='missing'; var_dump($$name ?? 1, isset($$name), empty($$name));
--EXPECT--
int(1)
bool(false)
bool(true)
