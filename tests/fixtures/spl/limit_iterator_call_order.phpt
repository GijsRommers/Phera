--TEST--
LimitIterator preserves the call order of user and seekable iterators
--FILE--
<?php
class UserRows implements Iterator {
    private int $index = 0;
    public function rewind(): void { echo "rewind\n"; $this->index = 0; }
    public function valid(): bool { echo "valid {$this->index}\n"; return $this->index < 5; }
    public function current(): mixed { echo "current {$this->index}\n"; return $this->index * 10; }
    public function key(): mixed { echo "key {$this->index}\n"; return 'r' . $this->index; }
    public function next(): void { echo "next {$this->index}\n"; ++$this->index; }
}
echo "-- bounded loop --\n";
$rows = new LimitIterator(new UserRows, 1, 2);
foreach ($rows as $key => $value) {
    echo "$key=$value\n";
}
echo "-- seek --\n";
echo $rows->seek(1), '=', $rows->getPosition(), ':', $rows->current(), "\n";
echo "-- seekable --\n";
class SeekableRows extends UserRows implements SeekableIterator {
    public function seek(int $offset): void {
        echo "seek $offset\n";
        parent::rewind();
        for ($i = 0; $i < $offset; ++$i) parent::next();
    }
}
$rows = new LimitIterator(new SeekableRows, 2, 1);
foreach ($rows as $value) { echo "value=$value\n"; }
--EXPECT--
-- bounded loop --
rewind
valid 0
next 0
valid 1
valid 1
current 1
key 1
r1=10
next 1
valid 2
current 2
key 2
r2=20
next 2
-- seek --
rewind
valid 0
next 0
valid 1
valid 1
current 1
key 1
1=1:10
-- seekable --
rewind
seek 2
rewind
next 0
next 1
valid 2
current 2
key 2
value=20
next 2
