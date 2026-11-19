--TEST--
A yielding getter creates a generator before reading its backing slot
--FILE--
<?php
class Source {
    public mixed $items = 4 {
        get {
            echo "getter body\n";
            yield $this->items;
            return 12;
        }
    }
    public Generator $short { get => yield 'short'; }
}
$source = new Source();
$name = 'items';
$stream = $source->$name;
echo get_class($stream), "\n";
var_dump(iterator_to_array($stream), $stream->getReturn());
var_dump(iterator_to_array($source->short));
--EXPECT--
Generator
getter body
array(1) {
  [0]=>
  int(4)
}
int(12)
array(1) {
  [0]=>
  string(5) "short"
}
