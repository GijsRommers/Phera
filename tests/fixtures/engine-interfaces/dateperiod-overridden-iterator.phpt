--TEST--
dateperiod overridden iterator
--FILE--
<?php
class EmptyCursor implements Iterator {
    public function rewind(): void { echo "rewind\n"; }
    public function valid(): bool { return false; }
    public function current(): mixed { return null; }
    public function key(): mixed { return null; }
    public function next(): void {}
}
class CuratedPeriod extends DatePeriod {
    public function getIterator(): Iterator { echo "override\n"; return new EmptyCursor; }
}
$p = new CuratedPeriod(new DateTime('2026-01-01 UTC'), new DateInterval('P1D'), 2);
foreach ($p as $value) { echo "ignored\n"; }
var_dump(iterator_count($p));
--EXPECT--
override
rewind
override
rewind
int(0)
