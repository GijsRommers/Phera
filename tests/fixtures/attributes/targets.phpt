--TEST--
Attribute instance creation validates the target, repetition, and attribute class
--FILE--
<?php
#[Attribute(Attribute::TARGET_METHOD)]
class MethodOnly {}
#[Attribute]
class Once {}
class Plain {}
#[MethodOnly, Once, Once, Plain, Unloaded(1 + 2)]
class Subject {}
$class = new ReflectionClass(Subject::class);
foreach ($class->getAttributes() as $attribute) {
    try {
        $attribute->newInstance();
    } catch (Error $error) {
        echo $error->getMessage(), "\n";
    }
}
var_dump($class->getAttributes('Unloaded')[0]->getArguments());
--EXPECT--
Attribute "MethodOnly" cannot target class (allowed targets: method)
Attribute "Once" must not be repeated
Attribute "Once" must not be repeated
Attempting to use non-attribute class "Plain" as attribute
Attribute class "Unloaded" not found
array(1) {
  [0]=>
  int(3)
}
