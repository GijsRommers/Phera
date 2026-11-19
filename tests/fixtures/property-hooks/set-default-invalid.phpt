--TEST--
A setter parameter cannot have a default value
--FILE--
<?php
class C {
    public int $x {
        set(int $value = 1) {}
    }
}
--EXPECTF--
Fatal error: Parameter $value of set hook C::$x must not have a default value in %s on line 4
