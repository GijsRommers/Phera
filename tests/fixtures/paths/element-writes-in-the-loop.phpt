--TEST--
Plain element writes give the same results as the general path
--FILE--
<?php
class Box
{
    public array $items = [];
    public ?array $maybe = null;
    public readonly array $fixed;
    private array $hidden = ['a' => 1];
    public $untyped = ['x' => ['y' => 1]];
    public function __construct() { $this->fixed = ['k' => 1]; }
    public function put(string $key, mixed $value): void { $this->items[$key] = $value; }
    public function hide(int $value): void { $this->hidden['a'] = $value; }
    public function hidden(): array { return $this->hidden; }
}

function locals(): void
{
    for ($round = 0; $round < 3; $round++) {
        $a = ['k' => 0, 'list' => [1, 2]];
        $copy = $a;
        $a['k'] = $round;                 // separates from $copy
        $a['list'][1] = 'two';            // nested, separates the inner array
        $a['new'] = true;                 // a new key at the end
        $a['10'] = 'ten';                 // numeric string key becomes 10
        $a[11] = 'eleven';
        $a['list'][5] = 'five';
        $kept = $a['k'] = $round * 10;    // the assignment's value
        echo json_encode($a), ' ', json_encode($copy), ' ', $kept, "\n";

        $ref = 1;
        $b = ['r' => &$ref];
        $b['r'] = 7 + $round;             // writes through the reference
        echo $ref, "\n";

        $c = ['k' => 1];
        foreach ($c as $key => $v) {
            $c['k'] = $v + 1;             // the loop keeps its own copy
            $c['other'] = $round;
        }
        echo json_encode($c), "\n";

        $d = [];
        $d['made']['here'] = $round;      // creates the level
        $e = null;
        $e['x'] = 1;                      // creates the array
        echo json_encode($d), json_encode($e), "\n";

        $s = 'abc';
        try {
            $s[1] = 'X';                  // a string offset
        } catch (Error $error) {
        }
        echo $s, "\n";
    }
}

function objects(): void
{
    $box = new Box;
    $alias = $box->items;
    for ($round = 0; $round < 3; $round++) {
        $box->items['k'] = $round;
        $box->items['n'][] = $round;
        $box->put('p', $round);
        $box->untyped['x']['y'] = $round;
        $box->hide($round);
        $kept = $box->items['z'] = $round;
        try {
            $box->fixed['k'] = 2;
        } catch (Error $error) {
            echo get_class($error), ': ', $error->getMessage(), "\n";
        }
        try {
            $box->maybe['k'] = $round;
        } catch (Error $error) {
            echo $error->getMessage(), "\n";
        }
        echo json_encode($box->items), json_encode($box->untyped), json_encode($box->hidden()), json_encode($box->maybe), json_encode($alias), $kept, "\n";
    }
    $other = new Box;
    for ($round = 0; $round < 2; $round++) {
        $target = $round === 0 ? $box : $other;   // another object, same class
        $target->items['k'] = 'o' . $round;
        echo json_encode($target->items), "\n";
    }
    $obj = new ArrayObject([]);
    $holder = new stdClass;
    $holder->list = ['a' => 1];
    for ($round = 0; $round < 2; $round++) {
        $holder->list['a'] = $round;
        $holder->dynamic['b'] = $round;
        echo json_encode($holder), "\n";
    }
}

locals();
objects();
--EXPECT--
{"k":0,"list":{"0":1,"1":"two","5":"five"},"new":true,"10":"ten","11":"eleven"} {"k":0,"list":[1,2]} 0
7
{"k":2,"other":0}
{"made":{"here":0}}{"x":1}
aXc
{"k":10,"list":{"0":1,"1":"two","5":"five"},"new":true,"10":"ten","11":"eleven"} {"k":0,"list":[1,2]} 10
8
{"k":2,"other":1}
{"made":{"here":1}}{"x":1}
aXc
{"k":20,"list":{"0":1,"1":"two","5":"five"},"new":true,"10":"ten","11":"eleven"} {"k":0,"list":[1,2]} 20
9
{"k":2,"other":2}
{"made":{"here":2}}{"x":1}
aXc
Error: Cannot indirectly modify readonly property Box::$fixed
{"k":0,"n":[0],"p":0,"z":0}{"x":{"y":0}}{"a":0}{"k":0}[]0
Error: Cannot indirectly modify readonly property Box::$fixed
{"k":1,"n":[0,1],"p":1,"z":1}{"x":{"y":1}}{"a":1}{"k":1}[]1
Error: Cannot indirectly modify readonly property Box::$fixed
{"k":2,"n":[0,1,2],"p":2,"z":2}{"x":{"y":2}}{"a":2}{"k":2}[]2
{"k":"o0","n":[0,1,2],"p":2,"z":2}
{"k":"o1"}
{"list":{"a":0},"dynamic":{"b":0}}
{"list":{"a":1},"dynamic":{"b":1}}
