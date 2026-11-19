--TEST--
A virtual property exposes only its declared operations
--FILE--
<?php
class C {
    public string $name { get => 'Ada'; }
    public int $number { set { echo $value, "\n"; } }
}
$c = new C;
$c->number = 42;
var_dump(property_exists($c, 'name'), property_exists(C::class, 'number'));
try { $c->name = 'Tess'; } catch (Error $e) { echo $e->getMessage(), "\n"; }
try { echo $c->number; } catch (Error $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
42
bool(true)
bool(true)
Property C::$name is read-only
Property C::$number is write-only
