--TEST--
Scalar temporaries preserve catch, finally, yield, and owned-value release order
--FILE--
<?php
class Held {
    function __construct(public string $name) {}
    function __destruct() { echo "drop ", $this->name, "\n"; }
}
function fail(): never { throw new RuntimeException('failure'); }
function pair($a, $b): void {}
function guarded(bool $choose): void {
    $first = [1, 2];
    try {
        echo $choose ? (int) 1.5 : (int) true, "\n";
        $second = $first;
        $second[0] = 3;
        echo $first[0], ' ', $second[0], "\n";
        pair(new Held('argument'), fail());
    } catch (RuntimeException $error) {
        echo 'caught ', $error->getMessage(), "\n";
    } finally {
        echo "finally\n";
    }
}
function suspended(): Generator {
    $held = new Held('generator');
    yield (int) 3.5;
    echo "resumed\n";
    unset($held);
}
function change(int &$referent): void {
    try {
        $referent = (int) 8.5;
    } finally {
        echo "reference finally\n";
    }
}
function coerced(): string {
    try {
        return 12;
    } finally {
        echo "return finally\n";
    }
}
set_error_handler(function ($kind, $message) {
    echo $message, "\n";
    return true;
});
echo (int) $missing, "\n";
guarded(true);
guarded(false);
$generator = suspended();
echo $generator->current(), "\n";
$generator->next();
var_dump(coerced());

$number = 4;
$alias =& $number;
change($alias);
echo $number, " ", $alias, "\n";
--EXPECT--
Undefined variable $missing
0
1
1 3
drop argument
caught failure
finally
1
1 3
drop argument
caught failure
finally
3
resumed
drop generator
return finally
string(2) "12"
reference finally
8 8
