--TEST--
Exceptions from PHP serialization hooks retain the native caller in the trace
--FILE--
<?php
class HookTrace {
    public function __serialize(): array { throw new RuntimeException('encode'); }
    public function __unserialize(array $value): void { throw new RuntimeException('decode'); }
}
foreach ([fn() => serialize(new HookTrace), fn() => unserialize('O:9:"HookTrace":0:{}')] as $run) {
    try { $run(); } catch (Throwable $e) {
        echo $e->getMessage(), "\n";
        $trace = $e->getTrace();
        echo $trace[0]['function'], " ", $trace[1]['function'], "\n";
    }
}
--EXPECT--
encode
__serialize serialize
decode
__unserialize unserialize
