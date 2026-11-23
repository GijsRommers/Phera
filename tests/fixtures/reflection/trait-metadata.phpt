--TEST--
Trait members retain original metadata in the consuming declaration
--FILE--
<?php
namespace Meta;
#[\Attribute(\Attribute::TARGET_ALL)]
class Mark {
    public function __construct(public string $name) {}
}
trait Inner {
    /** property */
    #[Mark(__CLASS__)]
    public int $n = 42;
    /** constant */
    #[Mark(__TRAIT__)]
    public const N = 42;
    /** method */
    #[Mark(__CLASS__)]
    public function run(#[Mark(__CLASS__)] int $n = 1) {}
}
trait Outer { use Inner { run as again; } }
class Base { public function root() {} }
class Child extends Base {
    use Outer { Outer::run as aliased; again as repeated; }
    public function local() {}
}
$r = new \ReflectionClass(Child::class);
echo implode(',', array_keys($r->getTraits())), "\n";
echo implode(',', $r->getTraitNames()), "\n";
foreach ($r->getTraitAliases() as $alias => $original) echo "$alias:$original\n";
foreach ($r->getMethods() as $m) {
    echo $m->name, ':', $m->getDeclaringClass()->name, ':', $m->getStartLine(), ':', $m->getEndLine(), "\n";
}
echo (int) (new \ReflectionClass(Outer::class))->isTrait(), "\n";
$m = $r->getMethod('repeated');
$p = $r->getProperty('n');
$c = $r->getReflectionConstant('N');
foreach ([$m, $m->getParameters()[0], $p, $c] as $member) {
    echo $member->getDeclaringClass()->name, ':', $member->getAttributes()[0]->newInstance()->name, "\n";
}
echo $p->getDocComment(), ':', $c->getDocComment(), ':', $m->getDocComment(), "\n";
echo (new \ReflectionClassConstant(Inner::class, 'N'))->getValue(), "\n";
--EXPECT--
Meta\Outer
Meta\Outer
aliased:Meta\Outer::run
repeated:Meta\Outer::again
local:Meta\Child:22:22
repeated:Meta\Child:16:16
again:Meta\Child:16:16
aliased:Meta\Child:16:16
run:Meta\Child:16:16
root:Meta\Base:19:19
1
Meta\Child:Meta\Child
Meta\Child:Meta\Child
Meta\Child:Meta\Child
Meta\Child:Meta\Inner
/** property */:/** constant */:/** method */
42
