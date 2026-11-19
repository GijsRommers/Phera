--TEST--
A synchronous native error still records its argument values
--FILE--
<?php
function splitField($text) { return explode('', $text); }
try {
    splitField('field');
} catch (ValueError $error) {
    echo $error->getMessage(), "\n";
    foreach ($error->getTrace() as $frame) { echo $frame['function'], "\n"; }
}
--EXPECT--
explode(): Argument #1 ($separator) must not be empty
explode
splitField
