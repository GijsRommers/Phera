--TEST--
Filters and append iterators honor subclass predicates and recursive children
--FILE--
<?php
class EvenRows extends FilterIterator {
    public function accept(): bool { return $this->current() % 2 === 0; }
}
class NoOddLeaves extends RecursiveFilterIterator {
    public function accept(): bool { return $this->hasChildren() || $this->current() % 2 === 0; }
}
$concat = new AppendIterator;
$concat->append(new EvenRows(new ArrayIterator([1, 2, 3])));
$concat->append(new RecursiveIteratorIterator(
    new NoOddLeaves(new RecursiveArrayIterator([[6, 7], 8]))
));
foreach ($concat as $value) {
    echo $concat->getIteratorIndex(), ':', $value, "\n";
}
--EXPECT--
0:2
1:6
1:8
