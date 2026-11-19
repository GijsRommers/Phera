--TEST--
Exact native methods retain overrides and ordinary argument checks
--FILE--
<?php
declare(strict_types=1);
class ExtraCount extends ArrayObject {
    public function count(): int { return parent::count() + 10; }
}
foreach ([new ArrayObject([1, 2]), new ExtraCount([1, 2])] as $object) {
    var_dump($object->count());
}
$date = new DateTimeImmutable('2026-01-01T00:00:00Z');
var_dump($date->format('Y'), $date->format(format: 'm'));
try { $date->format(2026); } catch (TypeError $error) { echo $error->getMessage(), "\n"; }
$reflection = new ReflectionClass(ArrayObject::class);
var_dump($reflection->getName());
try { $reflection->getName(1); } catch (ArgumentCountError $error) { echo $error->getMessage(), "\n"; }
--EXPECT--
int(2)
int(12)
string(4) "2026"
string(2) "01"
DateTimeImmutable::format(): Argument #1 ($format) must be of type string, int given
string(11) "ArrayObject"
ReflectionClass::getName() expects exactly 0 arguments, 1 given
