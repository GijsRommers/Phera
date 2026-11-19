--TEST--
Native SPL adapters and generator delegation preserve traversal keys and values
--FILE--
<?php
function rows(): Generator {
    yield 'a' => 2;
    yield 'b' => 4;
    yield 'c' => 6;
}

function show(string $label, iterable $rows): void {
    $parts = [];
    foreach ($rows as $key => $value) {
        $parts[] = "$key=$value";
    }
    echo $label, ' ', implode(',', $parts), "\n";
}

show('IteratorIterator', new IteratorIterator(rows()));
show('LimitIterator', new LimitIterator(rows(), 1, 1));
show('CallbackFilterIterator', new CallbackFilterIterator(rows(), fn ($value) => $value > 2));
$append = new AppendIterator;
try {
    $append->append(rows());
    show('AppendIterator', $append);
} catch (Throwable $error) {
    echo 'AppendIterator ', get_class($error), ': ', $error->getMessage(), "\n";
}
show('iterator_to_array', iterator_to_array(rows()));
echo 'iterator_count ', iterator_count(rows()), "\n";
$rows = rows();
$seen = [];
$count = iterator_apply($rows, function ($rows) use (&$seen) {
    $seen[] = $rows->current();
    return count($seen) < 2;
}, [$rows]);
echo 'iterator_apply ', $count, '|', implode(',', $seen), "\n";
$noRewind = new NoRewindIterator(rows());
$noRewind->next();
show('NoRewindIterator', $noRewind);
show('CachingIterator', new CachingIterator(rows()));

function fromArrayIterator(): Generator {
    yield from new ArrayIterator(['x' => 8, 'y' => 9]);
}
function fromArrayObject(): Generator {
    yield from new ArrayObject(['z' => 10, 'w' => 11]);
}
show('yield from ArrayIterator', fromArrayIterator());
show('yield from ArrayObject', fromArrayObject());
?>
--EXPECT--
IteratorIterator a=2,b=4,c=6
LimitIterator b=4
CallbackFilterIterator b=4,c=6
AppendIterator a=2,b=4,c=6
iterator_to_array a=2,b=4,c=6
iterator_count 3
iterator_apply 2|2,4
NoRewindIterator b=4,c=6
CachingIterator a=2,b=4,c=6
yield from ArrayIterator x=8,y=9
yield from ArrayObject z=10,w=11
