--TEST--
Skipping a parent's private slot leaves a child's private slot lazy
--FILE--
<?php
class ParentEntry {
    private int $id = 3;
    public function parentId(): int { return $this->id; }
}
class Entry extends ParentEntry {
    private int $id = 4;
    public function id(): int { return $this->id; }
}
$class = new ReflectionClass(Entry::class);
$entry = $class->newLazyGhost(static function () { echo "initializer
"; });
$parent = new ReflectionProperty(ParentEntry::class, 'id');
$parent->skipLazyInitialization($entry);
var_dump($parent->isLazy($entry), $class->getProperty('id')->isLazy($entry));
echo $entry->parentId(), "
";
echo $entry->id(), "
";
--EXPECT--
bool(false)
bool(true)
3
initializer
4
