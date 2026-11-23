--TEST--
Unsupported locale candidate types are diagnosed before applying candidates
--FILE--
<?php
try {
    setlocale(LC_ALL, 'C', new stdClass());
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
setlocale(): Argument #3 must be of type array|string|null, stdClass given
