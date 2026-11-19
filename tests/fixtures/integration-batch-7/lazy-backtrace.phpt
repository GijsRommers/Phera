--TEST--
A lazy ghost initializer has a backtrace
--FILE--
<?php
class Payload { public int $value; }
function initialize(Payload $object) {
    foreach (debug_backtrace(DEBUG_BACKTRACE_IGNORE_ARGS) as $frame) {
        printf("%s %d\n", $frame['function'], isset($frame['args']));
    }
    $object->value = 42;
}
function read(Payload $object) { return $object->value; }
$ghost = (new ReflectionClass(Payload::class))->newLazyGhost('initialize');
var_dump(read($ghost));
--EXPECT--
initialize 0
read 0
int(42)
