--TEST--
A jump out of a user Iterator abandons the traversal before re-entry
--FILE--
<?php
class Counter implements Iterator {
    private int $n = 0;
    function rewind(): void { echo "rewind\n"; $this->n = 0; }
    function valid(): bool { return $this->n < 2; }
    function current(): mixed { return $this->n; }
    function key(): mixed { return $this->n; }
    function next(): void { echo "next\n"; ++$this->n; }
}
$iterator = new Counter;
foreach ($iterator as $n) {
    echo $n, "\n";
    goto done;
}
done:
foreach ($iterator as $n) {
    echo $n, "\n";
}
--EXPECT--
rewind
0
rewind
0
next
1
next
