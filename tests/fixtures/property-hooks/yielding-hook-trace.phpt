--TEST--
An exception trace names a resumed generator getter as a hook
--FILE--
<?php
class Source {
    public Generator $value {
        get {
            yield 1;
            throw new RuntimeException('next failed');
        }
    }
}
$stream = (new Source())->value;
var_dump($stream->current());
try {
    $stream->next();
} catch (RuntimeException $error) {
    echo $error->getTraceAsString(), "\n";
}
--EXPECTF--
int(1)
#0 [internal function]: Source->$value::get()
#1 %s(13): Generator->next()
#2 {main}
