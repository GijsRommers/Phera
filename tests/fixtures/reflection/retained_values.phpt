--TEST--
Reflection objects retain their subjects without recursively freeing long chains
--FILE--
<?php
$reflector = new stdClass();
$secret = 'start';
for ($i = 0; $i < 18000; ++$i) {
    $reflector = new ReflectionObject($reflector);
    $secret = new SensitiveParameterValue($secret);
}
unset($reflector, $secret);
echo "released\n";
--EXPECT--
released
