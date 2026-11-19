--TEST--
A setter can accept a wider type and the getter uses the property return type
--FILE--
<?php
class C {
    public int $x {
        get => (string) $this->x;
        set(int|string $value) { $this->x = $value; }
    }
}
$c = new C;
$c->x = '42';
var_dump($c->x);
try { $c->x = 'bad'; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
int(42)
Cannot assign string to property C::$x of type int
