--TEST--
Reflection flattens iterable unions and preserves intersections and nullability
--FILE--
<?php
interface Left {}
interface Right {}
class Example {}
function declaration(iterable|Example $iterator, (Left&Right)|null $pair): Example|null { return null; }
$r = new ReflectionFunction('declaration');
foreach ($r->getParameters() as $parameter) {
    $type = $parameter->getType();
    echo get_class($type), ':', $type, ':', (int) $type->allowsNull(), "\n";
    foreach ($type->getTypes() as $member) echo get_class($member), ':', $member, "\n";
}
$type = $r->getReturnType();
echo get_class($type), ':', $type, ':', $type->getName(), ':', (int) $type->isBuiltin(), "\n";
--EXPECT--
ReflectionUnionType:Traversable|Example|array:0
ReflectionNamedType:Traversable
ReflectionNamedType:Example
ReflectionNamedType:array
ReflectionUnionType:(Left&Right)|null:1
ReflectionIntersectionType:Left&Right
ReflectionNamedType:null
ReflectionNamedType:?Example:Example:0
