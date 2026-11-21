--TEST--
Named class constants read in the loop: public, inherited, from interfaces and native classes; private, protected, deprecated, enum and missing ones through the VM
--FILE--
<?php
namespace App;
interface HasLimit { const LIMIT = 10; }
class Base implements HasLimit {
    const NAME = 'base';
    const DERIVED = self::NAME . '-derived';
    protected const HIDDEN = 'hidden';
    private const SECRET = 'secret';
    #[\Deprecated] const OLD = 'old';
    public static function inside() { return [self::HIDDEN, self::SECRET, static::NAME, Base::SECRET]; }
}
class Child extends Base { const NAME = 'child'; public function prot() { return Base::HIDDEN; } }
enum Status: string { case On = 'on'; const DEFAULT = self::On; }
function read() {
    return [Base::NAME, Base::DERIVED, Child::NAME, Child::LIMIT, HasLimit::LIMIT, \PDO::FETCH_ASSOC, Status::DEFAULT->value, Status::On->value];
}
for ($i = 0; $i < 3; $i++) { echo json_encode(read()), "\n"; }
echo json_encode(Base::inside()), "\n";
echo (new Child)->prot(), "\n";
echo Base::OLD, "\n";
try { echo Base::SECRET; } catch (\Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { echo Base::HIDDEN; } catch (\Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { echo Missing::X; } catch (\Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }
try { echo Base::NOPE; } catch (\Error $e) { echo get_class($e), ': ', $e->getMessage(), "\n"; }

--EXPECTF--
["base","base-derived","child",10,10,2,"on","on"]
["base","base-derived","child",10,10,2,"on","on"]
["base","base-derived","child",10,10,2,"on","on"]
["hidden","secret","base","secret"]
hidden

Deprecated: Constant App\Base::OLD is deprecated in %s on line 20
old
Error: Cannot access private constant App\Base::SECRET
Error: Cannot access protected constant App\Base::HIDDEN
Error: Class "App\Missing" not found
Error: Undefined constant App\Base::NOPE
