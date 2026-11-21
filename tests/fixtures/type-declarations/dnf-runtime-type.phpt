--TEST--
A DNF type keeps its intersection grouped in a type error
--FILE--
<?php
interface Readable {}
interface Writable {}
class ReadWrite implements Readable, Writable {}
function writeValue(int|(Readable&Writable) $value): void {
    echo get_debug_type($value), "\n";
}
writeValue(7);
writeValue(new ReadWrite());
try {
    writeValue(new stdClass());
} catch (TypeError $e) {
    echo explode(', called in ', $e->getMessage())[0], "\n";
}
--EXPECTF--
int
ReadWrite
writeValue(): Argument #1 ($value) must be of type (Readable&Writable)|int, stdClass given
