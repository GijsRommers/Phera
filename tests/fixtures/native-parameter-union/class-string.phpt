--TEST--
A native class union checks objects before trying its weak string arm
--FILE--
<?php
class ParentType {}
class ChildType extends ParentType {}
class TypeReflection extends ReflectionClass {
    public function __toString(): string {
        echo "unexpected coercion\n";
        return 'unused';
    }
}
class TypeName {
    public function __toString(): string {
        echo "getting class name\n";
        return ParentType::class;
    }
}
$child = new ReflectionClass(ChildType::class);
$type = (new ReflectionMethod(ReflectionClass::class, 'isSubclassOf'))->getParameters()[0]->getType();
echo $type, "\n";
var_dump($child->isSubclassOf(new TypeReflection(ParentType::class)));
var_dump($child->isSubclassOf(new TypeName()));
try {
    $child->isSubclassOf(new stdClass());
} catch (TypeError $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
ReflectionClass|string
bool(true)
getting class name
bool(true)
ReflectionClass::isSubclassOf(): Argument #1 ($class) must be of type ReflectionClass|string, stdClass given
