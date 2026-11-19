--TEST--
A synchronous native method error records the method frame
--FILE--
<?php
function missingMethod() { return (new ReflectionClass(stdClass::class))->getMethod('missing'); }
try {
    missingMethod();
} catch (ReflectionException $error) {
    $first = $error->getTrace()[0];
    var_dump($first['class'], $first['type'], $first['function']);
}
--EXPECT--
string(15) "ReflectionClass"
string(2) "->"
string(9) "getMethod"
