--TEST--
Exceptions raised by the native serialization functions include the built-in frame
--FILE--
<?php
class TypedCache { public int $count; }
foreach ([
    fn() => serialize(static fn() => 1),
    fn() => unserialize('O:10:"TypedCache":1:{s:5:"count";s:1:"x";}'),
] as $run) {
    try {
        $run();
    } catch (Throwable $error) {
        echo get_class($error), " ", $error->getMessage(), "\n";
        echo $error->getTrace()[0]['function'], "\n";
    }
}
--EXPECT--
Exception Serialization of 'Closure' is not allowed
serialize
TypeError Cannot assign string to property TypedCache::$count of type int
unserialize
