--TEST--
Foreach calls Iterator methods on the enum case
--FILE--
<?php
enum Days implements Iterator {
    case Weekend;
    private function &position(): int {
        static $position = 0;
        return $position;
    }
    public function rewind(): void { $position =& $this->position(); $position = 0; }
    public function valid(): bool { return $this->position() < 2; }
    public function current(): mixed { return ['Sat', 'Sun'][$this->position()]; }
    public function key(): mixed { return $this->position(); }
    public function next(): void { $position =& $this->position(); ++$position; }
}
foreach (Days::Weekend as $number => $day) {
    echo $number, ' ', $day, "\n";
}
--EXPECT--
0 Sat
1 Sun
