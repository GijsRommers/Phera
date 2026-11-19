--TEST--
An exception trace retains a hook frame called through a generator
--FILE--
<?php
class Source {
    public int $value { get => throw new RuntimeException('read failed'); }
    public function values(): Generator {
        yield $this->value;
    }
}
try {
    (new Source())->values()->current();
} catch (RuntimeException $error) {
    echo $error->getTraceAsString(), "\n";
}
--EXPECTF--
#0 %s(5): Source->$value::get()
#1 [internal function]: Source->values()
#2 %s(9): Generator->current()
#3 {main}
