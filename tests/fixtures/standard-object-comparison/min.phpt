--TEST--
min preserves the PHP operand order and first equal value
--EXTENSIONS--
gmp
--FILE--
<?php
$a = new DateTime('2020-01-01');
$b = new DateTime('2021-01-01');
$same = new DateTimeImmutable('2020-01-01');
var_dump(min($b, $a) === $a, min([$b, $a]) === $a);
$minimum = 'min';
var_dump($minimum($a, $same) === $a, min([$same, $a]) === $same);
var_dump(min([[$b], [$a]]) === [$a]);
$a = fn() => 1;
$b = fn() => 2;
var_dump($minimum($a, $b) === $a, min([$a, $b]) === $b);
final class BadText {
    function __toString(): string { throw new Exception('min string conversion'); }
}
try { min([new BadText, 'a']); }
catch (Exception $error) {
    echo $error->getMessage(), "\n";
    echo implode(',', array_column($error->getTrace(), 'function')), "\n";
}
$three = gmp_init(3);
var_dump(min(gmp_init(10), 3));
var_dump(min([[$three], [5]]) === [$three]);
var_dump(min([5], [$three]) === [$three]);
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
min string conversion
__toString,min
int(3)
bool(true)
bool(true)
