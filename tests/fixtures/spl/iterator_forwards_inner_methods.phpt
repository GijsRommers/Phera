--TEST--
SPL iterator adapters forward missing methods to the active inner iterator
--FILE--
<?php
class ForwardedEntries extends ArrayIterator {
    private function hidden($number) { return get_class($this) . ":$number"; }
    public function label() { return "inner label"; }
    public function __call($method, $args) { return "inner:" . $method . ":" . $args['name']; }
}
class OverrideForwarding extends LimitIterator {
    public function label() { return "wrapper label"; }
    public function __call($method, $args) { return "wrapper:" . $method . ":" . $args['name']; }
}
$inner = new ForwardedEntries([10, 20, 30]);
$limit = new LimitIterator($inner, 1, 1);
echo $limit->hidden(number: 7), "\n";
echo (new NoRewindIterator($limit))->label(), "\n";
echo (new CachingIterator($limit))->missing(name: "nested"), "\n";
echo (new OverrideForwarding($inner))->label(), "\n";
echo (new OverrideForwarding($inner))->missing(name: "override"), "\n";
try {
    (new IteratorIterator(new ArrayIterator))->missing();
} catch (Error $e) {
    echo $e->getMessage(), "\n";
}

class BranchEntries extends RecursiveArrayIterator {
    public function label() { return implode(",", array_keys($this->getArrayCopy())); }
}
$tree = new RecursiveIteratorIterator(new BranchEntries(['root' => ['leaf' => 42]]));
echo $tree->label(), "\n";
foreach ($tree as $value) {
    echo $tree->label(), " ", $value, "\n";
}
--EXPECT--
ForwardedEntries:7
inner label
inner:missing:nested
wrapper label
wrapper:missing:override
Call to undefined method IteratorIterator::missing()
root
leaf 42
