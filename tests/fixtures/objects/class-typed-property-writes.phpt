--TEST--
Objects written to class-, interface-, enum-, nullable-, union-, intersection-, self- and parent-typed properties and promoted parameters, with PHP's TypeError texts
--FILE--
<?php
interface Marker {} interface Other {}
class Plain implements Marker {} class Sub extends Plain {} class Both implements Marker, Other {}
final class Unrelated {}
enum S: string { case A = 'a'; case B = 'b'; }
enum T { case X; }
class Str { public function __toString(): string { return 'str'; } }
class Base {
    public self $me; public ?Base $maybe = null;
}
class Child extends Base { public parent $up; }
final class Holder {
    public Plain $p; public ?Marker $m = null; public S $s; public Plain|S|null $u = null;
    public Marker&Other $both; public object $obj; public DateTimeInterface $date; public string $text = '';
    public iterable $it = []; public Plain|int $pi = 0;
}
final class Promoted { public function __construct(public Plain $a, public ?Marker $b, public int $n) {} }
function attempt(string $label, callable $f) {
    try { $f(); echo "$label: ok\n"; } catch (TypeError $e) { echo "$label: ", $e->getMessage(), "\n"; }
}
$h = new Holder; $o = new Plain; $sub = new Sub; $both = new Both; $u = new Unrelated;
for ($i = 0; $i < 3; $i++) {
    attempt('p plain', fn () => $h->p = $o);
    attempt('p sub', fn () => $h->p = $sub);
    attempt('p unrelated', fn () => $h->p = $u);
    attempt('p null', fn () => $h->p = null);
    attempt('p enum', fn () => $h->p = S::A);
    attempt('m interface', fn () => $h->m = $both);
    attempt('m null', fn () => $h->m = null);
    attempt('m unrelated', fn () => $h->m = $u);
    attempt('s enum', fn () => $h->s = S::B);
    attempt('s other enum', fn () => $h->s = T::X);
    attempt('s string', fn () => $h->s = 'a');
    attempt('u class', fn () => $h->u = $sub);
    attempt('u enum', fn () => $h->u = S::A);
    attempt('u unrelated', fn () => $h->u = $u);
    attempt('both', fn () => $h->both = $both);
    attempt('both partial', fn () => $h->both = $o);
    attempt('obj', fn () => $h->obj = $u);
    attempt('obj int', fn () => $h->obj = 1);
    attempt('date', fn () => $h->date = new DateTimeImmutable('2020-01-01'));
    attempt('date wrong', fn () => $h->date = $o);
    attempt('text stringable', fn () => $h->text = new Str);
    attempt('it array', fn () => $h->it = [1]);
    attempt('it object', fn () => $h->it = new ArrayIterator([]));
    attempt('pi object', fn () => $h->pi = $o);
    attempt('pi int', fn () => $h->pi = 3);
    attempt('pi string', fn () => $h->pi = '4');
}
echo get_class($h->p), ' ', $h->s->value, ' ', get_class($h->u), ' ', $h->text, ' ', $h->pi, "\n";
$b = new Base; $c = new Child;
attempt('self', fn () => $b->me = $b);
attempt('self child', fn () => $b->me = $c);
attempt('self wrong', fn () => $b->me = $o);
attempt('parent', fn () => $c->up = $b);
attempt('parent wrong', fn () => $c->up = $o);
attempt('maybe', fn () => $b->maybe = $c);
for ($i = 0; $i < 2; $i++) {
    attempt('promoted', fn () => new Promoted($sub, $both, 1));
    attempt('promoted wrong', fn () => new Promoted($u, null, 1));
    attempt('promoted wrong 2', fn () => new Promoted($o, $u, 1));
}

--EXPECTF--
p plain: ok
p sub: ok
p unrelated: Cannot assign Unrelated to property Holder::$p of type Plain
p null: Cannot assign null to property Holder::$p of type Plain
p enum: Cannot assign S to property Holder::$p of type Plain
m interface: ok
m null: ok
m unrelated: Cannot assign Unrelated to property Holder::$m of type ?Marker
s enum: ok
s other enum: Cannot assign T to property Holder::$s of type S
s string: Cannot assign string to property Holder::$s of type S
u class: ok
u enum: ok
u unrelated: Cannot assign Unrelated to property Holder::$u of type Plain|S|null
both: ok
both partial: Cannot assign Plain to property Holder::$both of type Marker&Other
obj: ok
obj int: Cannot assign int to property Holder::$obj of type object
date: ok
date wrong: Cannot assign Plain to property Holder::$date of type DateTimeInterface
text stringable: ok
it array: ok
it object: ok
pi object: ok
pi int: ok
pi string: ok
p plain: ok
p sub: ok
p unrelated: Cannot assign Unrelated to property Holder::$p of type Plain
p null: Cannot assign null to property Holder::$p of type Plain
p enum: Cannot assign S to property Holder::$p of type Plain
m interface: ok
m null: ok
m unrelated: Cannot assign Unrelated to property Holder::$m of type ?Marker
s enum: ok
s other enum: Cannot assign T to property Holder::$s of type S
s string: Cannot assign string to property Holder::$s of type S
u class: ok
u enum: ok
u unrelated: Cannot assign Unrelated to property Holder::$u of type Plain|S|null
both: ok
both partial: Cannot assign Plain to property Holder::$both of type Marker&Other
obj: ok
obj int: Cannot assign int to property Holder::$obj of type object
date: ok
date wrong: Cannot assign Plain to property Holder::$date of type DateTimeInterface
text stringable: ok
it array: ok
it object: ok
pi object: ok
pi int: ok
pi string: ok
p plain: ok
p sub: ok
p unrelated: Cannot assign Unrelated to property Holder::$p of type Plain
p null: Cannot assign null to property Holder::$p of type Plain
p enum: Cannot assign S to property Holder::$p of type Plain
m interface: ok
m null: ok
m unrelated: Cannot assign Unrelated to property Holder::$m of type ?Marker
s enum: ok
s other enum: Cannot assign T to property Holder::$s of type S
s string: Cannot assign string to property Holder::$s of type S
u class: ok
u enum: ok
u unrelated: Cannot assign Unrelated to property Holder::$u of type Plain|S|null
both: ok
both partial: Cannot assign Plain to property Holder::$both of type Marker&Other
obj: ok
obj int: Cannot assign int to property Holder::$obj of type object
date: ok
date wrong: Cannot assign Plain to property Holder::$date of type DateTimeInterface
text stringable: ok
it array: ok
it object: ok
pi object: ok
pi int: ok
pi string: ok
Sub b S str 4
self: ok
self child: ok
self wrong: Cannot assign Plain to property Base::$me of type Base
parent: ok
parent wrong: Cannot assign Plain to property Child::$up of type Base
maybe: ok
promoted: ok
promoted wrong: Promoted::__construct(): Argument #1 ($a) must be of type Plain, Unrelated given, called in %s on line 60
promoted wrong 2: Promoted::__construct(): Argument #2 ($b) must be of type ?Marker, Unrelated given, called in %s on line 61
promoted: ok
promoted wrong: Promoted::__construct(): Argument #1 ($a) must be of type Plain, Unrelated given, called in %s on line 60
promoted wrong 2: Promoted::__construct(): Argument #2 ($b) must be of type ?Marker, Unrelated given, called in %s on line 61
