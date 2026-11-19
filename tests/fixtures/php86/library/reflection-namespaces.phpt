--TEST--
Namespace introspection for attributes and global constants
--FILE--
<?php
namespace Demo;
const LOCAL = true;
#[Example, \GlobalAttribute]
class Subject {}
foreach ((new \ReflectionClass(Subject::class))->getAttributes() as $attribute) {
    echo $attribute->getName(), PHP_EOL;
    if (method_exists($attribute, 'inNamespace')) {
        var_dump($attribute->inNamespace(), $attribute->getNamespaceName(), $attribute->getShortName());
    }
}
foreach (['Demo\\LOCAL', 'PHP_VERSION'] as $name) {
    $constant = new \ReflectionConstant($name);
    if (method_exists($constant, 'inNamespace')) {
        var_dump($constant->inNamespace());
    }
}
--EXPECT--
Demo\Example
bool(true)
string(4) "Demo"
string(7) "Example"
GlobalAttribute
bool(false)
string(0) ""
string(15) "GlobalAttribute"
bool(true)
bool(false)
