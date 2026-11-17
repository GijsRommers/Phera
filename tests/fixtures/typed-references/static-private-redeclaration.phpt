--TEST--
Typed reference bookkeeping: inherited private static storage
--FILE--
<?php
class P {
    private static int $n = 1;
    public static function &get() { return self::$n; }
}
class C extends P { public static string $n = "child"; }
$parent =& C::get();
$child =& C::$n;
$child = "updated";
try { $parent = []; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
var_dump($child, $parent);
$new = "next";
C::$n =& $new;
$child = [];
try { $new = []; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
var_dump($child, C::$n);
--EXPECT--
Cannot assign array to reference held by property P::$n of type int
string(7) "updated"
int(1)
Cannot assign array to reference held by property C::$n of type string
array(0) {
}
string(4) "next"
