--TEST--
Factory-only native classes expose no constructor and keep their allocation error
--FILE--
<?php
$reflection = new ReflectionClass(Directory::class);
var_dump($reflection->getConstructor(), $reflection->isInstantiable());
$name = $reflection->getName();
foreach ([fn () => new Directory(), fn () => new $name(), fn () => $reflection->newInstance()] as $create) {
    try { $create(); } catch (Error $error) { echo $error->getMessage(), "\n"; }
}
$directory = dir(__DIR__);
var_dump(get_class($directory));
$directory->close();
--EXPECT--
NULL
bool(true)
Cannot directly construct Directory, use dir() instead
Cannot directly construct Directory, use dir() instead
Cannot directly construct Directory, use dir() instead
string(9) "Directory"
