--TEST--
spl_object_id and strtolower under strict_types: operands that are not accepted raise TypeError, as PHP does
--FILE--
<?php
declare(strict_types=1);
namespace App;
class Stringy { function __toString(): string { return 'X'; } }
function show(string $label, callable $f): void {
    try { $r = $f(); echo "$label: ", var_export($r, true), "\n"; }
    catch (\Throwable $e) { echo "$label: ", get_class($e), ': ', $e->getMessage(), "\n"; }
}
show('strtolower string', fn () => strtolower('ABC'));
show('strtolower int', fn () => strtolower(5));
show('strtolower null', fn () => strtolower(null));
show('strtolower object with __toString', fn () => strtolower(new Stringy));
show('spl_object_id int', fn () => spl_object_id(5));
show('spl_object_id object', fn () => is_int(spl_object_id(new \stdClass)));
show('qualified strtolower int', fn () => \strtolower(5));
show('implode ints', fn () => implode(',', [1, 2]));
show('int concatenation', fn () => 'a' . 5 . 6);

--EXPECT--
strtolower string: 'abc'
strtolower int: TypeError: strtolower(): Argument #1 ($string) must be of type string, int given
strtolower null: TypeError: strtolower(): Argument #1 ($string) must be of type string, null given
strtolower object with __toString: TypeError: strtolower(): Argument #1 ($string) must be of type string, App\Stringy given
spl_object_id int: TypeError: spl_object_id(): Argument #1 ($object) must be of type object, int given
spl_object_id object: true
qualified strtolower int: TypeError: strtolower(): Argument #1 ($string) must be of type string, int given
implode ints: '1,2'
int concatenation: 'a56'
