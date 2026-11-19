--TEST--
The shared cursor handles generator iteration and both spreads
--FILE--
<?php
function args(): Iterator {
    yield 0 => 'positional';
    yield 'named' => 'named argument';
}
function join_args($position, $named) {
    echo "$position / $named\n";
}
class Aggregated implements IteratorAggregate {
    public function getIterator(): Traversable { return args(); }
}
var_dump([...args()]);
join_args(...args());
var_dump(iterator_count(args()));
var_dump(iterator_apply(args(), function () { echo "step\n"; return true; }));
var_dump(iterator_to_array(new Aggregated));
$generator = args();
foreach ($generator as $value) { echo "$value\n"; break; }
$generator->next();
var_dump($generator->current());
--EXPECT--
array(2) {
  [0]=>
  string(10) "positional"
  ["named"]=>
  string(14) "named argument"
}
positional / named argument
int(2)
step
step
int(2)
array(2) {
  [0]=>
  string(10) "positional"
  ["named"]=>
  string(14) "named argument"
}
positional
string(14) "named argument"
