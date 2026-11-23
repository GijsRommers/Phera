--TEST--
Classes: self:: and parent:: constant reads in methods use the named form, with PHP's values and messages
--FILE--
<?php
class P {
    protected const PROT = 2;
    public const PUB = 3;
    #[Deprecated(message: 'use PUB')] public const DEP = 4;
    const K = 5;
    const L = self::K + 1;
}
class C extends P {
    const K = 10;
    public function selfK() { return self::K; }
    public function parentK() { return parent::K; }
    public function selfL() { return self::L; }
    public function selfProt() { return self::PROT; }
    public function selfMissing() { return self::MISSING; }
    public function parentMissing() { return parent::MISSING; }
    public function selfDep() { return self::DEP; }
    public function selfClass() { return self::class; }
    public function parentClass() { return parent::class; }
    public function inClosure() { $f = function () { return self::K; }; return $f(); }
    public function inStaticClosure() { $f = static function () { return self::K; }; return $f(); }
    public function inArrow() { $f = fn () => self::K; return $f(); }
    public function boundClosure() { $f = function () { return self::K; }; return Closure::bind($f, null, 'D')(); }
}
class D { const K = 42; }
$c = new C;
echo $c->selfK(), ' ', $c->parentK(), ' ', $c->selfL(), ' ', $c->selfProt(), "\n";
echo $c->selfClass(), ' ', $c->parentClass(), "\n";
echo $c->inClosure(), ' ', $c->inStaticClosure(), ' ', $c->inArrow(), ' ', $c->boundClosure(), "\n";
foreach (['selfMissing', 'parentMissing'] as $m) {
    try { $c->$m(); } catch (\Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
}
echo $c->selfDep(), "\n";
class WithDefault { const K = 9; public int $x = self::K; }
echo (new WithDefault)->x, "\n";
interface I { const A = 1; const B = self::A + 1; }
echo I::B, "\n";
trait T { const TK = 12; public function tRead() { return self::TK; } }
class UsesT { use T; }
echo (new UsesT)->tRead(), "\n";
$anon = new class { const K = 11; public function f() { return self::K; } };
echo $anon->f(), "\n";
--EXPECTF--
10 5 6 2
C P
10 10 10 42
Error: Undefined constant C::MISSING
Error: Undefined constant P::MISSING

Deprecated: Constant P::DEP is deprecated, use PUB in %s on line 17
4
9
2
12
11
