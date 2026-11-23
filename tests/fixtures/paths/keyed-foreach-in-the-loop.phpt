--TEST--
Keyed foreach over arrays gives the same results as the general path
--FILE--
<?php
class Visible { public $a = 1; protected $b = 2; private $c = 3; public $d = [4];
    public function inside(): array { $out = []; foreach ($this as $k => $v) { $out[] = "$k=" . json_encode($v); } return $out; } }
class Noisy { public function __destruct() { echo "destruct\n"; } }

function gen() { yield 'x' => 1; yield 'y' => 2; }

function run(): void
{
    for ($round = 0; $round < 2; $round++) {
        $map = ['a' => 1, '10' => 'ten', 5 => 'five', 'b' => [1, 2], 'c' => null];
        $seen = [];
        foreach ($map as $key => $value) {
            $seen[] = var_export($key, true) . ':' . json_encode($value);
            $map['added'] = true;          // the loop keeps its own copy
            unset($map['b']);
        }
        echo implode(' ', $seen), ' | ', json_encode($map), "\n";

        foreach ([] as $key => $value) {
            echo "never\n";
        }
        $k = 'before';
        foreach (['only' => 1] as $k => $v) {
        }
        echo $k, $v, "\n";

        $out = [];
        foreach ([3 => 'c', 1 => 'a', 2 => 'b'] as $i => $letter) {
            foreach (['x', 'y'] as $j => $inner) {
                if ($j === 1 && $i === 1) {
                    break 2;
                }
                $out[] = "$i$letter$j$inner";
            }
        }
        echo implode(',', $out), "\n";

        echo implode(',', (new Visible)->inside()), "\n";
        $outside = [];
        foreach (new Visible as $name => $value) {
            $outside[] = $name;
        }
        echo implode(',', $outside), "\n";

        foreach (gen() as $name => $value) {
            echo "$name$value ";
        }
        echo "\n";

        $ref = ['p' => 1, 'q' => 2];
        foreach ($ref as $name => &$value) {
            $value *= 10;
        }
        unset($value);
        echo json_encode($ref), "\n";

        $objects = ['first' => new Noisy, 'second' => 1];
        foreach ($objects as $name => $value) {
            echo $name, "\n";
            $objects = null;           // the loop still owns the array
        }
        echo "after loop\n";
        unset($value, $name);
        echo "end of round\n";
    }
}

run();
--EXPECT--
'a':1 10:"ten" 5:"five" 'b':[1,2] 'c':null | {"a":1,"10":"ten","5":"five","c":null,"added":true}
only1
3c0x,3c1y,1a0x
a=1,b=2,c=3,d=[4]
a,d
x1 y2 
{"p":10,"q":20}
first
second
destruct
after loop
end of round
'a':1 10:"ten" 5:"five" 'b':[1,2] 'c':null | {"a":1,"10":"ten","5":"five","c":null,"added":true}
only1
3c0x,3c1y,1a0x
a=1,b=2,c=3,d=[4]
a,d
x1 y2 
{"p":10,"q":20}
first
second
destruct
after loop
end of round
