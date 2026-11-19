--TEST--
Generator serialization and unserialization report PHP errors before and after closing
--FILE--
<?php
function values(): Generator { yield 1; }
$generator = values();
foreach ([false, true] as $close) {
    if ($close) foreach ($generator as $value) {}
    try { serialize($generator); }
    catch (Throwable $error) { echo get_class($error), ' ', $error->getMessage(), "
"; }
}
try { unserialize('O:9:"Generator":0:{}'); }
catch (Throwable $error) { echo get_class($error), ' ', $error->getMessage(), "
"; }
--EXPECT--
Exception Serialization of 'Generator' is not allowed
Exception Serialization of 'Generator' is not allowed
Exception Unserialization of 'Generator' is not allowed
