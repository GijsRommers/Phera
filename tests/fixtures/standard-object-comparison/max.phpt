--TEST--
max uses native, Stringable, enum and nested comparisons
--EXTENSIONS--
gmp
--FILE--
<?php
$a = new DateTime('2020-01-01');
$b = new DateTime('2021-01-01');
$same = new DateTimeImmutable('2021-01-01');
var_dump(max($a, $b) === $b, max([$a, $b]) === $b);
var_dump(max($b, $same) === $b, max([$same, $b]) === $same);
var_dump(max([$a], [$b]) === [$b], max([[$a], [$b]]) === [$b]);
final class Text { function __toString(): string { return 'b'; } }
$text = new Text;
var_dump(max($text, 'b') === $text, max(['b', $text]) === 'b');
var_dump(max([[$text], ['a']]) === [$text]);
enum Choice { case First; case Second; }
$maximum = 'max';
var_dump($maximum(Choice::First, Choice::Second) === Choice::Second);
var_dump(max([Choice::First, Choice::Second]) === Choice::First);
$seven = gmp_init(7);
var_dump(max(gmp_init(3), 5));
var_dump(max([$seven, 2]) === $seven);
var_dump(max([[5], [$seven]]) === [$seven]);
var_dump(max([$seven], [5]) === [$seven]);
--EXPECT--
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
bool(true)
int(5)
bool(true)
bool(true)
bool(true)
