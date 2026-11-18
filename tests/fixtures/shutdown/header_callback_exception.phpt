--TEST--
The first output survives an exception from the header callback
--FILE--
<?php
header_register_callback(fn () => throw new Exception('failed'));
try { echo 'output'; } catch (Throwable $e) { echo ' caught ', $e->getMessage(); }
echo ' end';
--EXPECT--
output caught failed end
