--TEST--
A suspended method retains its receiver, scope, arguments and statics
--FILE--
<?php
class ParentSource {
    const LABEL = 'parent';
    private string $prefix = 'item';
    public function read($start = 1, ...$rest): iterable {
        static $count = 0;
        yield [$this->prefix, static::LABEL, ++$count, func_get_args(), $rest];
    }
}
class ChildSource extends ParentSource { const LABEL = 'child'; }
$source = new ChildSource;
$first = $source->read(start: 7, suffix: '!');
$second = $source->read(8, 9);
var_dump($second->current(), $first->current());
--EXPECT--
array(5) {
  [0]=>
  string(4) "item"
  [1]=>
  string(5) "child"
  [2]=>
  int(1)
  [3]=>
  array(2) {
    [0]=>
    int(8)
    [1]=>
    int(9)
  }
  [4]=>
  array(1) {
    [0]=>
    int(9)
  }
}
array(5) {
  [0]=>
  string(4) "item"
  [1]=>
  string(5) "child"
  [2]=>
  int(2)
  [3]=>
  array(1) {
    [0]=>
    int(7)
  }
  [4]=>
  array(1) {
    ["suffix"]=>
    string(1) "!"
  }
}
