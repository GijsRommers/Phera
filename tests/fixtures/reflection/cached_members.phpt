--TEST--
Reflection shares declarations while object properties and static values remain live
--FILE--
<?php
#[AllowDynamicProperties]
class CachedMembers {
    public int $number = 1;
    public static int $shared = 2;
}
$first = new CachedMembers();
$second = new CachedMembers();
$class = new ReflectionClass(CachedMembers::class);
$a = new ReflectionObject($first);
$b = new ReflectionObject($second);
$first->extra = 'first';
echo implode(',', array_map(fn ($p) => $p->getName(), $a->getProperties())), "\n";
echo implode(',', array_map(fn ($p) => $p->getName(), $b->getProperties())), "\n";
unset($first->extra);
$second->extra = 'second';
var_dump($a->hasProperty('extra'), $b->getProperty('extra')->getValue($second));
$number = $class->getProperty('number');
$number->setValue($first, 17);
var_dump($number->getValue($first), $number->getValue($second));
$class->setStaticPropertyValue('shared', 23);
var_dump($class->getProperty('shared')->getValue());
--EXPECT--
number,shared,extra
number,shared
bool(false)
string(6) "second"
int(17)
int(1)
int(23)
