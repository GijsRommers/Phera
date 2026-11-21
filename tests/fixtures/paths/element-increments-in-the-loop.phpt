--TEST--
Increments of elements give the same results as the general path
--FILE--
<?php
class Stats
{
    public array $counts = ['a' => 0];
    public float $f = 0.5;
    public ?int $n = null;
    public string $s = 'a';
    public array $nested = ['x' => ['y' => 1.5]];
    public function hit(string $key): int { return ++$this->counts[$key]; }
}
function run(): void
{
    $st = new Stats;
    for ($round = 0; $round < 3; $round++) {
        $words = ['a' => 0, 'b' => 1.5, 'c' => null, 'd' => 'z', 'e' => PHP_INT_MAX, 'f' => '9', 'g' => [1]];
        $copy = $words;
        $x = $words['a']++;
        $y = ++$words['a'];
        $words['b']--;
        $words['c']++;
        $words['c2'] = null;
        $words['c2']--;
        $words['d']++;
        $words['e']++;
        $words['f']++;
        try { $words['g']++; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
        $words['list'][0] = 1;
        $words['list'][0]--;
        echo $x, $y, ' ', json_encode($words), json_encode($copy), "\n";
        echo $st->hit('a'), ' ';
        $st->f++;
        $st->n++;
        $st->s++;
        $st->nested['x']['y']--;
        $st->counts['a']--;
        echo json_encode(get_object_vars($st)), "\n";
    }
    $u = [];
    $u['nope']++;
    echo json_encode($u), "\n";
}
run();
--EXPECTF--

Warning: Decrement on type null has no effect, this will change in the next major version of PHP in %s on line 22

Deprecated: Increment on non-numeric string is deprecated, use str_increment() instead in %s on line 23
Cannot increment array
02 {"a":2,"b":0.5,"c":1,"d":"aa","e":9.223372036854776e+18,"f":10,"g":[1],"c2":null,"list":[0]}{"a":0,"b":1.5,"c":null,"d":"z","e":9223372036854775807,"f":"9","g":[1]}
1 
Deprecated: Increment on non-numeric string is deprecated, use str_increment() instead in %s on line 33
{"counts":{"a":0},"f":1.5,"n":1,"s":"b","nested":{"x":{"y":0.5}}}

Warning: Decrement on type null has no effect, this will change in the next major version of PHP in %s on line 22

Deprecated: Increment on non-numeric string is deprecated, use str_increment() instead in %s on line 23
Cannot increment array
02 {"a":2,"b":0.5,"c":1,"d":"aa","e":9.223372036854776e+18,"f":10,"g":[1],"c2":null,"list":[0]}{"a":0,"b":1.5,"c":null,"d":"z","e":9223372036854775807,"f":"9","g":[1]}
1 
Deprecated: Increment on non-numeric string is deprecated, use str_increment() instead in %s on line 33
{"counts":{"a":0},"f":2.5,"n":2,"s":"c","nested":{"x":{"y":-0.5}}}

Warning: Decrement on type null has no effect, this will change in the next major version of PHP in %s on line 22

Deprecated: Increment on non-numeric string is deprecated, use str_increment() instead in %s on line 23
Cannot increment array
02 {"a":2,"b":0.5,"c":1,"d":"aa","e":9.223372036854776e+18,"f":10,"g":[1],"c2":null,"list":[0]}{"a":0,"b":1.5,"c":null,"d":"z","e":9223372036854775807,"f":"9","g":[1]}
1 
Deprecated: Increment on non-numeric string is deprecated, use str_increment() instead in %s on line 33
{"counts":{"a":0},"f":3.5,"n":3,"s":"d","nested":{"x":{"y":-1.5}}}

Warning: Undefined array key "nope" in %s on line 39
{"nope":1}
