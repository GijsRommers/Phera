--TEST--
JsonSerializable callbacks keep the internal stack frame
--FILE--
<?php
class WrappedValue implements JsonSerializable {
    public function jsonSerialize(): mixed {
        throw new Exception('callback failed');
    }
}
try {
    json_encode(['nested' => new WrappedValue()]);
} catch (Exception $error) {
    echo str_replace(__FILE__, 'script', $error->getTraceAsString()), "\n";
}
--EXPECT--
#0 [internal function]: WrappedValue->jsonSerialize()
#1 script(8): json_encode(Array)
#2 {main}
