--TEST--
foreach throws current
--FILE--
<?php
class Items implements Iterator {
    public $keyValue = "a";
    public $throwAt = "";
    public $position = 0;
    public $size = 2;
    private function observe(string $name): void {
        echo $name, "\n";
        if ($this->throwAt === $name) { throw new Exception($name); }
    }
    public function rewind(): void { $this->observe("rewind"); $this->position = 0; }
    public function valid(): bool { $this->observe("valid"); return $this->position < $this->size; }
    public function current(): mixed { $this->observe("current"); return $this->position + 11; }
    public function key(): mixed { $this->observe("key"); return $this->position === 0 ? $this->keyValue : 2; }
    public function next(): void { $this->observe("next"); ++$this->position; }
}
$it = new Items;
$it->throwAt = "current"; try { foreach ($it as $k=>$v) { echo "body\n"; } } catch(Exception $e) { echo "caught ", $e->getMessage(), "\n"; }
--EXPECT--
rewind
valid
current
caught current
