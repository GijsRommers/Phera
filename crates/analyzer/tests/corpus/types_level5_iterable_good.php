<?php
// Rule: PHP iterable signatures accept Traversable as well as arrays.
function checkedIterable(): void {
    echo iterator_count(new ArrayIterator([1, 2]));
}
