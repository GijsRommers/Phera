--TEST--
An ordinary getter can return a generator from a function call
--FILE--
<?php
function values(): Generator {
    echo "function body\n";
    yield 'a' => 7;
}
class Source {
    public Generator $stream {
        get {
            echo "getter\n";
            return values();
        }
    }
}
$stream = (new Source())->stream;
echo "after read\n";
var_dump(iterator_to_array($stream));
--EXPECT--
getter
after read
function body
array(1) {
  ["a"]=>
  int(7)
}
