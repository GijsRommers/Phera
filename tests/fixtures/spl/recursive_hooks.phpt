--TEST--
RecursiveIteratorIterator dispatches hooks with the active subiterator and depth
--FILE--
<?php
class LoggedWalk extends RecursiveIteratorIterator {
    public function beginIteration(): void { echo "begin\n"; }
    public function beginChildren(): void { echo 'children+', $this->getDepth(), "\n"; }
    public function endChildren(): void { echo 'children-', $this->getDepth(), "\n"; }
    public function nextElement(): void { echo 'at ', $this->getDepth(), "\n"; }
    public function callHasChildren(): bool { return parent::callHasChildren(); }
    public function callGetChildren(): ?RecursiveIterator { return parent::callGetChildren(); }
    public function endIteration(): void { echo "end\n"; }
}
$rows = new LoggedWalk(new RecursiveArrayIterator([[2, 3], 5]));
foreach ($rows as $value) {
    echo $value, "\n";
}
--EXPECT--
begin
children+1
at 1
2
at 1
3
children-1
at 0
5
end
