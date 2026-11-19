--TEST--
Iterator adapters respect trait-provided protocol methods
--FILE--
<?php
trait SuppliesIterator {
    public function getIterator(): Traversable {
        echo "getIterator\n";
        return new UppercaseIterator(['a' => 'one', 'b' => 'two', 'c' => 'three']);
    }
}
trait UppercaseCurrent {
    public function current(): mixed {
        return strtoupper(parent::current());
    }
}
class UppercaseIterator extends ArrayIterator {
    use UppercaseCurrent;
}
class Items implements IteratorAggregate {
    use SuppliesIterator;
}

$iterator = new LimitIterator(new IteratorIterator(new Items()), 1, 2);
foreach ($iterator as $key => $value) {
    echo $key, ' ', $value, "\n";
}
--EXPECT--
getIterator
b TWO
c THREE
