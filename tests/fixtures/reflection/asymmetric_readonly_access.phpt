--TEST--
Readonly initialization honors explicit and implicit setter visibility
--FILE--
<?php
class Sample {
    public private(set) readonly int $private;
    public readonly int $protected;
    public public(set) readonly int $public;
}
$sample = (new ReflectionClass(Sample::class))->newInstanceWithoutConstructor();
foreach (['private', 'protected', 'public'] as $name) {
    try { $sample->$name = 7; echo $sample->$name, "\n"; }
    catch (Error $error) { echo $error->getMessage(), "\n"; }
}
try { $sample->public = 8; }
catch (Error $error) { echo $error->getMessage(), "\n"; }
--EXPECT--
Cannot modify private(set) property Sample::$private from global scope
Cannot modify protected(set) readonly property Sample::$protected from global scope
7
Cannot modify readonly property Sample::$public
