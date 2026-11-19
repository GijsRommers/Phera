--TEST--
A raw write can initialize another lazy slot through string conversion
--FILE--
<?php
class Entry {
    public string $label;
    public string $value;
}
class Label {
    public function __construct(private Entry $entry) {}
    public function __toString(): string { return $this->entry->value; }
}
$class = new ReflectionClass(Entry::class);
$entry = $class->newLazyGhost(static function (Entry $entry) {
    echo "initializer
";
    $entry->value = 'loaded';
});
$class->getProperty('label')->setRawValueWithoutLazyInitialization($entry, new Label($entry));
var_dump($entry->label, $class->isUninitializedLazyObject($entry));
--EXPECT--
initializer
string(6) "loaded"
bool(false)
