--TEST--
Anonymous classes expose their source and class attributes
--FILE--
<?php
namespace Meta;
#[\Attribute]
class Mark {
    public function __construct(public string $name) {}
}
class ParentClass {}
$object = new #[Mark('anon')] /** anonymous class */ class extends ParentClass {
    public function hello(): string { return 'hi'; }
};
$r = new \ReflectionObject($object);
echo (int) $r->isAnonymous(), ':', (int) $r->inNamespace(), ':', $r->getNamespaceName(), "\n";
echo explode("\0", $r->getShortName())[0], ':', $r->getParentClass()->name, "\n";
echo $r->getStartLine(), ':', $r->getEndLine(), ':', $r->getDocComment(), "\n";
echo $r->getAttributes()[0]->newInstance()->name, ':', $r->getMethod('hello')->invoke($object), "\n";
$r = new \ReflectionClass(new class {});
echo (int) $r->isAnonymous(), ':', (int) $r->inNamespace(), ':', explode("\0", $r->getShortName())[0], "\n";
--EXPECT--
1:1:Meta
ParentClass@anonymous:Meta\ParentClass
8:10:/** anonymous class */
anon:hi
1:0:class@anonymous
