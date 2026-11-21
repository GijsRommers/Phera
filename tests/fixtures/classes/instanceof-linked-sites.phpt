--TEST--
instanceof gives the same answers once its class name is linked
--FILE--
<?php
interface Base {}
interface Left extends Base {}
interface Right extends Base {}
interface Both extends Left, Right {}
trait Helper {}
class Model implements Both { use Helper; }
class User extends Model {}
class_alias(User::class, 'Member');
enum Status { case On; }

// A wide interface tree: more ancestors than fit the loop's walk.
$names = [];
for ($i = 0; $i < 40; $i++) {
    eval("interface Wide$i {}");
    $names[] = "Wide$i";
}
eval('class Broad implements ' . implode(', ', $names) . ' {}');

function check(mixed $value): string
{
    return implode('', [
        (int) ($value instanceof Base),
        (int) ($value instanceof Left),
        (int) ($value instanceof Right),
        (int) ($value instanceof Both),
        (int) ($value instanceof Model),
        (int) ($value instanceof User),
        (int) ($value instanceof Member),
        (int) ($value instanceof Helper),
        (int) ($value instanceof Status),
        (int) ($value instanceof Wide0),
        (int) ($value instanceof Wide39),
        (int) ($value instanceof Later),
        (int) ($value instanceof Missing),
        (int) ($value instanceof stdClass),
        (int) ($value instanceof \UnitEnum),
        (int) ($value instanceof Countable),
    ]);
}

$values = [new User, new Model, Status::On, new Broad, new stdClass, new ArrayObject([]), 'User', null, 3];
// The first round links the names, the second answers from the links.
for ($round = 0; $round < 2; $round++) {
    foreach ($values as $value) {
        echo check($value), "\n";
    }
    if ($round === 0) {
        eval('class Later extends User {}');
        $values[] = new Later;
    }
}

class Scope extends Model
{
    public function check(object $value): string
    {
        return implode('', [
            (int) ($value instanceof self),
            (int) ($value instanceof static),
            (int) ($value instanceof parent),
        ]);
    }
}
class Inner extends Scope {}
for ($round = 0; $round < 2; $round++) {
    echo (new Scope)->check(new Inner), ' ', (new Inner)->check(new Scope), ' ', (new Scope)->check(new User), "\n";
}
--EXPECT--
1111111000000000
1111100000000000
0000000010000010
0000000001100000
0000000000000100
0000000000000001
0000000000000000
0000000000000000
0000000000000000
1111111000000000
1111100000000000
0000000010000010
0000000001100000
0000000000000100
0000000000000001
0000000000000000
0000000000000000
0000000000000000
1111111000010000
111 101 001
111 101 001
