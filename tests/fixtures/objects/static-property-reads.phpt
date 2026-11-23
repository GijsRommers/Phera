--TEST--
Static property reads by a named class, self::, static:: and parent::, served by the loop's site after the first: visibility, redeclared statics, typed without a value, references, elements, isset, ??, empty, closures bound to other scopes, a trait's static, a lazily evaluated default
--FILE--
<?php
// Static property reads by a named class, self::, static:: and parent::, each
// run several times so that the loop's site serves the later ones.
class Registry {
    public static $instance = null;
    protected static array $resolved = ['a' => 1, 'n' => null, 'deep' => ['x' => 'y']];
    private static $secret = 'registry secret';
    public static int $typed;
    public static $counter = 0;
    public static function instance() { return static::$instance; }
    public static function resolved($k) { return static::$resolved[$k] ?? 'missing'; }
    public static function deep() { return self::$resolved['deep']['x']; }
    public static function has($k) { return isset(self::$resolved[$k]) ? 'set' : 'not set'; }
    public static function emptyOf($k) { return empty(static::$resolved[$k]) ? 'empty' : 'full'; }
    public static function secret() { return self::$secret; }
    public static function typed() { try { return self::$typed; } catch (Error $e) { return $e->getMessage(); } }
    public static function counter() { return static::$counter; }
    public static function plain($k) { return static::$resolved[$k]; }
}
class Child extends Registry {
    public static $instance = 'child redeclared';
    public static function parentInstance() { return parent::$instance; }
    public static function secretFromChild() { try { return self::$secret; } catch (Error $e) { return $e->getMessage(); } }
    public static function protectedFromChild() { return self::$resolved['a']; }
}
class Other extends Registry {}
for ($round = 0; $round < 3; $round++) {
    echo implode(' | ', [
        var_export(Registry::instance(), true), var_export(Child::instance(), true), var_export(Other::instance(), true),
        Child::parentInstance(), Registry::resolved('a'), Child::resolved('b'), var_export(Other::resolved('n'), true),
        Registry::deep(), Registry::has('a'), Registry::has('n'), Registry::has('zz'),
        Registry::emptyOf('a'), Registry::emptyOf('n'), Registry::emptyOf('zz'),
        Registry::secret(), Child::secretFromChild(), Child::protectedFromChild(), Registry::typed(),
    ]), "\n";
    Registry::$instance = "set in round $round";
    Registry::$counter++;
    Registry::$typed = $round;
    echo Registry::counter(), ' ', Child::counter(), ' ', Other::counter(), ' ', Registry::$counter, ' ', Child::$counter, "\n";
}
// From outside: public reads; private and protected ones fail every time.
for ($round = 0; $round < 2; $round++) {
    echo Registry::$instance, ' | ', Child::$instance, ' | ', Registry::$typed, "\n";
    foreach (['secret', 'resolved'] as $name) {
        try { echo $name === 'secret' ? Registry::$secret : Registry::$resolved, "\n"; } catch (Error $e) { echo $e->getMessage(), "\n"; }
    }
}
// A reference bound to a static: reads see the other variable's writes.
$outside = 'first';
Registry::$counter = &$outside;
for ($round = 0; $round < 2; $round++) { echo Registry::counter(), ' '; $outside = "changed $round"; }
echo Registry::counter(), "\n";
// A missing element in a plain read warns, every time.
for ($round = 0; $round < 2; $round++) { echo var_export(Child::plain('zz'), true), "\n"; }
// Closures bound to different scopes read through the same code.
$read = function () { return self::$secret; };
$inRegistry = Closure::bind($read, null, Registry::class);
for ($round = 0; $round < 2; $round++) {
    echo $inRegistry(), "\n";
    try { echo Closure::bind($read, null, Other::class)(), "\n"; } catch (Error $e) { echo $e->getMessage(), "\n"; }
}
// A trait's static, read on the trait (deprecated) and on its user.
trait Counts { public static $hits = 5; public static function hits() { return self::$hits; } }
class UsesCounts { use Counts; }
for ($round = 0; $round < 2; $round++) { echo UsesCounts::hits(), ' ', UsesCounts::$hits, ' ', Counts::$hits, "\n"; }
// A default that is evaluated on first use.
class Lazy { public static $when = PHP_INT_SIZE * 2; public static function when() { return static::$when; } }
for ($round = 0; $round < 2; $round++) { echo Lazy::when(), "\n"; }
--EXPECTF--
NULL | 'child redeclared' | NULL |  | 1 | missing | 'missing' | y | set | not set | not set | full | empty | empty | registry secret | Cannot access private property Child::$secret | 1 | Typed static property Registry::$typed must not be accessed before initialization
1 1 1 1 1
'set in round 0' | 'child redeclared' | 'set in round 0' | set in round 0 | 1 | missing | 'missing' | y | set | not set | not set | full | empty | empty | registry secret | Cannot access private property Child::$secret | 1 | 0
2 2 2 2 2
'set in round 1' | 'child redeclared' | 'set in round 1' | set in round 1 | 1 | missing | 'missing' | y | set | not set | not set | full | empty | empty | registry secret | Cannot access private property Child::$secret | 1 | 1
3 3 3 3 3
set in round 2 | child redeclared | 2
Cannot access private property Registry::$secret
Cannot access protected property Registry::$resolved
set in round 2 | child redeclared | 2
Cannot access private property Registry::$secret
Cannot access protected property Registry::$resolved
first changed 0 changed 1

Warning: Undefined array key "zz" in %s on line 18
NULL

Warning: Undefined array key "zz" in %s on line 18
NULL
registry secret
Cannot access private property Other::$secret
registry secret
Cannot access private property Other::$secret
5 5 
Deprecated: Accessing static trait property Counts::$hits is deprecated, it should only be accessed on a class using the trait in %s on line 64
5
5 5 
Deprecated: Accessing static trait property Counts::$hits is deprecated, it should only be accessed on a class using the trait in %s on line 64
5
16
16
