--TEST--
Iterator helper callbacks keep the internal stack frame
--FILE--
<?php
class BrokenIterator implements Iterator {
    public function rewind(): void {}
    public function valid(): bool { return true; }
    public function current(): mixed { return 'value'; }
    public function key(): mixed { throw new Exception('callback failed'); }
    public function next(): void {}
}
try {
    iterator_to_array(new BrokenIterator());
} catch (Exception $error) {
    echo str_replace(__FILE__, 'script', $error->getTraceAsString()), "\n";
}
--EXPECT--
#0 [internal function]: BrokenIterator->key()
#1 script(10): iterator_to_array(Object(BrokenIterator))
#2 {main}
