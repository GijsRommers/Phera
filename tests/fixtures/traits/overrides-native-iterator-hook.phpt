--TEST--
A trait override replaces the inherited DatePeriod iterator hook
--FILE--
<?php
trait EmptyPeriod {
    public function getIterator(): Iterator {
        echo "trait\n";
        return new class implements Iterator {
            public function rewind(): void {}
            public function valid(): bool { return false; }
            public function current(): mixed { return null; }
            public function key(): mixed { return null; }
            public function next(): void {}
        };
    }
}
class Period extends DatePeriod { use EmptyPeriod; }
$period = new Period(new DateTimeImmutable('2026-01-01'), new DateInterval('P1D'), 1);
foreach ($period as $date) { echo $date->format('Y-m-d'), "\n"; }
echo "done\n";
--EXPECT--
trait
done
