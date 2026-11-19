--TEST--
Arguments, properties and return values convert to int, float, string and bool as PHP does without strict_types
--FILE--
<?php
function i(int $a) { return $a; } function is(int|string $a) { return $a; } function ib(int|bool $a) { return $a; } function f(float $a) { return $a; } function inull(?int $a) { return $a; } function ifl(int|float $a) { return $a; } function s(string $a) { return $a; } function b(bool $a) { return $a; }
class P { public int $i; public int|string $is; public ?float $f; }
$values = [1.5, 2.0, -0.0, "3", "3.0", "3.7", " 5", "5 ", "5x", "abc", "", 1e30, -1e30, NAN, INF, "1e30", "1e2", "0x1A", true, false, null, 9223372036854775807, 9.2233720368547758E+18, "9223372036854775808", [], "007", "-.5e1"];
foreach (['i', 'is', 'ib', 'f', 'inull', 'ifl', 's', 'b'] as $fn) {
    foreach ($values as $v) { echo $fn, '(', is_float($v) ? var_export($v, true) : json_encode($v), ') = '; try { var_dump($fn($v)); } catch (TypeError $e) { echo 'TypeError: ', preg_replace('/, called in.*/', '', $e->getMessage()), "\n"; } }
}
$p = new P;
foreach (['i', 'is', 'f'] as $prop) { foreach ([1.5, "3.7", "abc", 1e30, true, "1e2", 7] as $v) { echo $prop, ' <- ', json_encode($v), ' = '; try { $p->$prop = $v; var_dump($p->$prop); } catch (TypeError $e) { echo 'TypeError: ', $e->getMessage(), "\n"; } } }
function r(): int { return "4.5"; } var_dump(r());
--EXPECTF--
i(1.5) = 
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line %d
int(1)
i(2.0) = int(2)
i(-0.0) = int(0)
i("3") = int(3)
i("3.0") = int(3)
i("3.7") = 
Deprecated: Implicit conversion from float-string "3.7" to int loses precision in %s on line %d
int(3)
i(" 5") = int(5)
i("5 ") = int(5)
i("5x") = TypeError: i(): Argument #1 ($a) must be of type int, string given
i("abc") = TypeError: i(): Argument #1 ($a) must be of type int, string given
i("") = TypeError: i(): Argument #1 ($a) must be of type int, string given
i(1.0E+30) = TypeError: i(): Argument #1 ($a) must be of type int, float given
i(-1.0E+30) = TypeError: i(): Argument #1 ($a) must be of type int, float given
i(NAN) = TypeError: i(): Argument #1 ($a) must be of type int, float given
i(INF) = TypeError: i(): Argument #1 ($a) must be of type int, float given
i("1e30") = TypeError: i(): Argument #1 ($a) must be of type int, string given
i("1e2") = int(100)
i("0x1A") = TypeError: i(): Argument #1 ($a) must be of type int, string given
i(true) = int(1)
i(false) = int(0)
i(null) = TypeError: i(): Argument #1 ($a) must be of type int, null given
i(9223372036854775807) = int(9223372036854775807)
i(9.223372036854776E+18) = TypeError: i(): Argument #1 ($a) must be of type int, float given
i("9223372036854775808") = TypeError: i(): Argument #1 ($a) must be of type int, string given
i([]) = TypeError: i(): Argument #1 ($a) must be of type int, array given
i("007") = int(7)
i("-.5e1") = int(-5)
is(1.5) = 
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line %d
int(1)
is(2.0) = int(2)
is(-0.0) = int(0)
is("3") = string(1) "3"
is("3.0") = string(3) "3.0"
is("3.7") = string(3) "3.7"
is(" 5") = string(2) " 5"
is("5 ") = string(2) "5 "
is("5x") = string(2) "5x"
is("abc") = string(3) "abc"
is("") = string(0) ""
is(1.0E+30) = string(7) "1.0E+30"
is(-1.0E+30) = string(8) "-1.0E+30"
is(NAN) = 
Warning: unexpected NAN value was coerced to string in %s on line %d
string(3) "NAN"
is(INF) = string(3) "INF"
is("1e30") = string(4) "1e30"
is("1e2") = string(3) "1e2"
is("0x1A") = string(4) "0x1A"
is(true) = int(1)
is(false) = int(0)
is(null) = TypeError: is(): Argument #1 ($a) must be of type string|int, null given
is(9223372036854775807) = int(9223372036854775807)
is(9.223372036854776E+18) = string(19) "9.2233720368548E+18"
is("9223372036854775808") = string(19) "9223372036854775808"
is([]) = TypeError: is(): Argument #1 ($a) must be of type string|int, array given
is("007") = string(3) "007"
is("-.5e1") = string(5) "-.5e1"
ib(1.5) = 
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line %d
int(1)
ib(2.0) = int(2)
ib(-0.0) = int(0)
ib("3") = int(3)
ib("3.0") = int(3)
ib("3.7") = 
Deprecated: Implicit conversion from float-string "3.7" to int loses precision in %s on line %d
int(3)
ib(" 5") = int(5)
ib("5 ") = int(5)
ib("5x") = bool(true)
ib("abc") = bool(true)
ib("") = bool(false)
ib(1.0E+30) = bool(true)
ib(-1.0E+30) = bool(true)
ib(NAN) = 
Warning: unexpected NAN value was coerced to bool in %s on line %d
bool(true)
ib(INF) = bool(true)
ib("1e30") = bool(true)
ib("1e2") = int(100)
ib("0x1A") = bool(true)
ib(true) = bool(true)
ib(false) = bool(false)
ib(null) = TypeError: ib(): Argument #1 ($a) must be of type int|bool, null given
ib(9223372036854775807) = int(9223372036854775807)
ib(9.223372036854776E+18) = bool(true)
ib("9223372036854775808") = bool(true)
ib([]) = TypeError: ib(): Argument #1 ($a) must be of type int|bool, array given
ib("007") = int(7)
ib("-.5e1") = int(-5)
f(1.5) = float(1.5)
f(2.0) = float(2)
f(-0.0) = float(-0)
f("3") = float(3)
f("3.0") = float(3)
f("3.7") = float(3.7)
f(" 5") = float(5)
f("5 ") = float(5)
f("5x") = TypeError: f(): Argument #1 ($a) must be of type float, string given
f("abc") = TypeError: f(): Argument #1 ($a) must be of type float, string given
f("") = TypeError: f(): Argument #1 ($a) must be of type float, string given
f(1.0E+30) = float(1.0E+30)
f(-1.0E+30) = float(-1.0E+30)
f(NAN) = float(NAN)
f(INF) = float(INF)
f("1e30") = float(1.0E+30)
f("1e2") = float(100)
f("0x1A") = TypeError: f(): Argument #1 ($a) must be of type float, string given
f(true) = float(1)
f(false) = float(0)
f(null) = TypeError: f(): Argument #1 ($a) must be of type float, null given
f(9223372036854775807) = float(9.223372036854776E+18)
f(9.223372036854776E+18) = float(9.223372036854776E+18)
f("9223372036854775808") = float(9.223372036854776E+18)
f([]) = TypeError: f(): Argument #1 ($a) must be of type float, array given
f("007") = float(7)
f("-.5e1") = float(-5)
inull(1.5) = 
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line %d
int(1)
inull(2.0) = int(2)
inull(-0.0) = int(0)
inull("3") = int(3)
inull("3.0") = int(3)
inull("3.7") = 
Deprecated: Implicit conversion from float-string "3.7" to int loses precision in %s on line %d
int(3)
inull(" 5") = int(5)
inull("5 ") = int(5)
inull("5x") = TypeError: inull(): Argument #1 ($a) must be of type ?int, string given
inull("abc") = TypeError: inull(): Argument #1 ($a) must be of type ?int, string given
inull("") = TypeError: inull(): Argument #1 ($a) must be of type ?int, string given
inull(1.0E+30) = TypeError: inull(): Argument #1 ($a) must be of type ?int, float given
inull(-1.0E+30) = TypeError: inull(): Argument #1 ($a) must be of type ?int, float given
inull(NAN) = TypeError: inull(): Argument #1 ($a) must be of type ?int, float given
inull(INF) = TypeError: inull(): Argument #1 ($a) must be of type ?int, float given
inull("1e30") = TypeError: inull(): Argument #1 ($a) must be of type ?int, string given
inull("1e2") = int(100)
inull("0x1A") = TypeError: inull(): Argument #1 ($a) must be of type ?int, string given
inull(true) = int(1)
inull(false) = int(0)
inull(null) = NULL
inull(9223372036854775807) = int(9223372036854775807)
inull(9.223372036854776E+18) = TypeError: inull(): Argument #1 ($a) must be of type ?int, float given
inull("9223372036854775808") = TypeError: inull(): Argument #1 ($a) must be of type ?int, string given
inull([]) = TypeError: inull(): Argument #1 ($a) must be of type ?int, array given
inull("007") = int(7)
inull("-.5e1") = int(-5)
ifl(1.5) = float(1.5)
ifl(2.0) = float(2)
ifl(-0.0) = float(-0)
ifl("3") = int(3)
ifl("3.0") = float(3)
ifl("3.7") = float(3.7)
ifl(" 5") = int(5)
ifl("5 ") = int(5)
ifl("5x") = TypeError: ifl(): Argument #1 ($a) must be of type int|float, string given
ifl("abc") = TypeError: ifl(): Argument #1 ($a) must be of type int|float, string given
ifl("") = TypeError: ifl(): Argument #1 ($a) must be of type int|float, string given
ifl(1.0E+30) = float(1.0E+30)
ifl(-1.0E+30) = float(-1.0E+30)
ifl(NAN) = float(NAN)
ifl(INF) = float(INF)
ifl("1e30") = float(1.0E+30)
ifl("1e2") = float(100)
ifl("0x1A") = TypeError: ifl(): Argument #1 ($a) must be of type int|float, string given
ifl(true) = int(1)
ifl(false) = int(0)
ifl(null) = TypeError: ifl(): Argument #1 ($a) must be of type int|float, null given
ifl(9223372036854775807) = int(9223372036854775807)
ifl(9.223372036854776E+18) = float(9.223372036854776E+18)
ifl("9223372036854775808") = float(9.223372036854776E+18)
ifl([]) = TypeError: ifl(): Argument #1 ($a) must be of type int|float, array given
ifl("007") = int(7)
ifl("-.5e1") = float(-5)
s(1.5) = string(3) "1.5"
s(2.0) = string(1) "2"
s(-0.0) = string(2) "-0"
s("3") = string(1) "3"
s("3.0") = string(3) "3.0"
s("3.7") = string(3) "3.7"
s(" 5") = string(2) " 5"
s("5 ") = string(2) "5 "
s("5x") = string(2) "5x"
s("abc") = string(3) "abc"
s("") = string(0) ""
s(1.0E+30) = string(7) "1.0E+30"
s(-1.0E+30) = string(8) "-1.0E+30"
s(NAN) = 
Warning: unexpected NAN value was coerced to string in %s on line %d
string(3) "NAN"
s(INF) = string(3) "INF"
s("1e30") = string(4) "1e30"
s("1e2") = string(3) "1e2"
s("0x1A") = string(4) "0x1A"
s(true) = string(1) "1"
s(false) = string(0) ""
s(null) = TypeError: s(): Argument #1 ($a) must be of type string, null given
s(9223372036854775807) = string(19) "9223372036854775807"
s(9.223372036854776E+18) = string(19) "9.2233720368548E+18"
s("9223372036854775808") = string(19) "9223372036854775808"
s([]) = TypeError: s(): Argument #1 ($a) must be of type string, array given
s("007") = string(3) "007"
s("-.5e1") = string(5) "-.5e1"
b(1.5) = bool(true)
b(2.0) = bool(true)
b(-0.0) = bool(false)
b("3") = bool(true)
b("3.0") = bool(true)
b("3.7") = bool(true)
b(" 5") = bool(true)
b("5 ") = bool(true)
b("5x") = bool(true)
b("abc") = bool(true)
b("") = bool(false)
b(1.0E+30) = bool(true)
b(-1.0E+30) = bool(true)
b(NAN) = 
Warning: unexpected NAN value was coerced to bool in %s on line %d
bool(true)
b(INF) = bool(true)
b("1e30") = bool(true)
b("1e2") = bool(true)
b("0x1A") = bool(true)
b(true) = bool(true)
b(false) = bool(false)
b(null) = TypeError: b(): Argument #1 ($a) must be of type bool, null given
b(9223372036854775807) = bool(true)
b(9.223372036854776E+18) = bool(true)
b("9223372036854775808") = bool(true)
b([]) = TypeError: b(): Argument #1 ($a) must be of type bool, array given
b("007") = bool(true)
b("-.5e1") = bool(true)
i <- 1.5 = 
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line %d
int(1)
i <- "3.7" = 
Deprecated: Implicit conversion from float-string "3.7" to int loses precision in %s on line %d
int(3)
i <- "abc" = TypeError: Cannot assign string to property P::$i of type int
i <- 1.0e+30 = TypeError: Cannot assign float to property P::$i of type int
i <- true = int(1)
i <- "1e2" = int(100)
i <- 7 = int(7)
is <- 1.5 = 
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line %d
int(1)
is <- "3.7" = string(3) "3.7"
is <- "abc" = string(3) "abc"
is <- 1.0e+30 = string(7) "1.0E+30"
is <- true = int(1)
is <- "1e2" = string(3) "1e2"
is <- 7 = int(7)
f <- 1.5 = float(1.5)
f <- "3.7" = float(3.7)
f <- "abc" = TypeError: Cannot assign string to property P::$f of type ?float
f <- 1.0e+30 = float(1.0E+30)
f <- true = float(1)
f <- "1e2" = float(100)
f <- 7 = float(7)

Deprecated: Implicit conversion from float-string "4.5" to int loses precision in %s on line %d
int(4)
