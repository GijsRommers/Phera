--TEST--
Native class unions check inheritance and reflect their named members
--FILE--
<?php
class Rows extends RecursiveArrayIterator {}
class RowSource implements IteratorAggregate {
    public function getIterator(): Traversable { return new Rows(['second']); }
}
$constructor = new ReflectionMethod(RecursiveTreeIterator::class, '__construct');
$type = $constructor->getParameters()[0]->getType();
echo get_class($type), ': ', implode('|', array_map(fn ($type) => $type->getName(), $type->getTypes())), "\n";
foreach ([new Rows(['first']), new RowSource()] as $source) {
    foreach (new RecursiveTreeIterator($source) as $row) {
        echo $row, "\n";
    }
}
--EXPECT--
ReflectionUnionType: RecursiveIterator|IteratorAggregate
\-first
\-second
