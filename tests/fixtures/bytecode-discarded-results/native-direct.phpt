--TEST--
A discarded direct native result preserves binding errors in a protected range
--FILE--
<?php
function discardNative($text): void {
    try {
        strtoupper($text);
        echo "native call completed\n";
    } catch (TypeError $error) {
        echo $error->getMessage(), "\n";
    }
}
discardNative('hello');
discardNative([]);
--EXPECT--
native call completed
strtoupper(): Argument #1 ($string) must be of type string, array given
