--TEST--
The hook body uses declaring-file strict types for backing assignments
--FILE--
<?php
declare(strict_types=1);
class C {
    public int $x {
        set(int|string $value) { $this->x = $value; }
    }
}
$c = new C;
try { $c->x = '42'; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
Cannot assign string to property C::$x of type int
