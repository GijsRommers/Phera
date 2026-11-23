--TEST--
Property exists members and visibility
--FILE--
<?php
trait Helper { public int $traitProp = 7; }
class Base {
    use Helper;
    public int $pub = 1;
    protected int $prot = 2;
    private int $priv = 3;
    public static string $stat = 's';
    public int $unsetMe = 4;
    public int $uninit;
}
class Child extends Base {}
enum Suit: string { case Hearts = 'h'; }
$base = new Base;
$std = new stdClass;
$std->dyn = 1;
var_dump(property_exists($base, 'pub'));
var_dump(property_exists($base, 'prot'));
var_dump(property_exists($base, 'priv'));
var_dump(property_exists($base, 'stat'));
var_dump(property_exists($base, 'traitProp'));
var_dump(property_exists($base, 'uninit'));
var_dump(property_exists($base, 'nope'));
var_dump(property_exists('Base', 'priv'));
var_dump(property_exists('Child', 'priv'));
var_dump(property_exists('Nope', 'p'));
var_dump(property_exists($std, 'dyn'));
unset($std->dyn);
var_dump(property_exists($std, 'dyn'));
unset($base->unsetMe);
var_dump(property_exists($base, 'unsetMe'));
var_dump(property_exists(Suit::Hearts, 'name'));
var_dump(property_exists(Suit::Hearts, 'value'));
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(false)
bool(true)
bool(false)
bool(false)
bool(true)
bool(false)
bool(true)
bool(true)
bool(true)
