--TEST--
Enum reflectors expose cases, backing values and case attributes
--FILE--
<?php
#[Attribute(Attribute::TARGET_CLASS | Attribute::TARGET_CLASS_CONSTANT)]
class Route {
    public function __construct(public array $methods = []) {}
}
interface Tagged {}
#[Route(methods: [Method::Get])]
enum Method: string implements Tagged {
    /** HTTP GET */
    #[Route([self::Post])]
    case Get = 'GET';
    public const DEFAULT = self::Get;
    case Post = 'POST';
}
enum State { case Ready; }
$r = new ReflectionEnum(Method::Get);
echo $r->getName(), ':', $r->getBackingType(), ':', implode(',', $r->getInterfaceNames()), "\n";
echo $r->getAttributes()[0]->newInstance()->methods[0]->value, "\n";
foreach ($r->getReflectionConstants() as $c) {
    echo $c->name, ':', (int) $c->isEnumCase(), ':', $c->getDocComment(), "\n";
}
foreach ($r->getCases() as $c) {
    echo get_class($c), ':', $c->getEnum()->name, ':', $c->getBackingValue(), "\n";
}
$c = new ReflectionEnumBackedCase(Method::class, 'Get');
$a = $c->getAttributes()[0];
echo $a->getTarget(), ':', $a->getArguments()[0][0]->name, ':', (int) ($c->getValue() === Method::Get), "\n";
$r = new ReflectionEnum(State::class);
$c = $r->getCase('Ready');
echo (int) $r->isBacked(), (int) $r->hasCase('READY'), ':', get_class($c), ':', $c->getValue()->name, "\n";
foreach ([
    fn() => new ReflectionEnum(Route::class),
    fn() => new ReflectionEnumBackedCase(State::class, 'Ready'),
    fn() => $r->getCase('Missing'),
    fn() => new ReflectionEnumUnitCase(Method::class, 'DEFAULT'),
    fn() => (new ReflectionEnum(Method::class))->getCase('DEFAULT'),
] as $f) {
    try { $f(); } catch (ReflectionException $e) { echo $e->getMessage(), "\n"; }
}
--EXPECT--
Method:string:Tagged,UnitEnum,BackedEnum
GET
Get:1:/** HTTP GET */
DEFAULT:0:
Post:1:
ReflectionEnumBackedCase:Method:GET
ReflectionEnumBackedCase:Method:POST
16:Post:1
00:ReflectionEnumUnitCase:Ready
Class "Route" is not an enum
Enum case State::Ready is not a backed case
Case State::Missing does not exist
Constant Method::DEFAULT is not a case
Method::DEFAULT is not a case
