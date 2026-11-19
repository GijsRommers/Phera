--TEST--
WeakReference and WeakMap retain the proxy identity after initialization
--FILE--
<?php
class Entry { public int $id = 5; }
$class = new ReflectionClass(Entry::class);
$entry = $class->newLazyProxy(static fn () => new Entry());
$weak = WeakReference::create($entry);
$map = new WeakMap();
$map[$entry] = 'proxy';
$original = $class->initializeLazyObject($entry);
var_dump($weak->get() === $entry, $weak->get() === $original);
var_dump($map[$entry], isset($map[$original]));
--EXPECT--
bool(true)
bool(false)
string(5) "proxy"
bool(false)
