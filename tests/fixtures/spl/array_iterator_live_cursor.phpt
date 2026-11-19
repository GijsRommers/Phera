--TEST--
ArrayIterator shares its live cursor across nested loops and gives clones a fresh cursor
--FILE--
<?php
$i = new ArrayIterator(['a', 'b', 'c']);
foreach ($i as $key => $value) {
    echo "$key=$value\n";
    if ($key === 0) {
        unset($i[0]);
        $i[] = 'd';
        $copy = clone $i;
        echo 'clone ', $copy->key(), '=', $copy->current(), "\n";
    }
    if ($key === 2) {
        foreach ($i as $nestedKey => $nestedValue) {
            echo "inner $nestedKey=$nestedValue\n";
        }
    }
}
--EXPECT--
0=a
clone 1=b
2=c
inner 1=b
inner 2=c
inner 3=d
