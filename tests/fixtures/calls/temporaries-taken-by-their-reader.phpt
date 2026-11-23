--TEST--
Temporaries and literals read once by their reader: sends to lean natives, references, destructor order of taken receivers
--FILE--
<?php
class Noisy
{
    public function __construct(public string $name) {}
    public function __destruct() { echo "destruct {$this->name}\n"; }
    public function name(): string { echo "in {$this->name}\n"; return $this->name; }
    public function self(): static { return $this; }
}
class Holder
{
    private array $seen = [];
    public function merge(string $key, array $values): array
    {
        // `?? []` sent to a lean native: the general path must describe it.
        $this->seen[$key] = array_merge($this->seen[$key] ?? [], $values);
        return $this->seen;
    }
}
class NoisyError extends Exception
{
    public function __destruct() { echo "destruct error {$this->getMessage()}\n"; }
}
function make(string $name): Noisy { return new Noisy($name); }
function fail(string $message): NoisyError { return new NoisyError($message, 7); }
function pair($a, $b) { return "$a/$b"; }
function byRef(&$x) { $x .= '!'; }

function run(): void
{
    for ($round = 0; $round < 2; $round++) {
        $holder = new Holder();
        $holder->merge('x', [1]);
        echo json_encode($holder->merge('x', [2])), "\n";
        echo pair('lit', 2.5), ' ', str_repeat('ab', 2), ' ', implode(',', ['a', 'b']), "\n";
        $s = 'local';
        $t = 'other';
        $r = &$t;
        $r = 'through';
        echo $s, ' ', $t, "\n";
        byRef($s);
        echo $s, "\n";
        echo 'got ', make('one')->name(), "\n";
        $sum = make('two')->name() . make('three')->name();
        echo "sum $sum\n";
        echo make('four')->self()->name(), "\n";
        // Native methods and native getters of a temporary receiver.
        echo fail('first')->getMessage(), "\n";
        echo fail('second')->getCode() + 1, "\n";
        echo (new ArrayObject([1, 2, 3]))->count(), "\n";
        echo (new DateTimeImmutable('2020-02-03'))->format('Y-m-d'), "\n";
        echo "end of round $round\n";
    }
    try {
        byRef('literal');
    } catch (Error $e) {
        echo get_class($e), ': ', $e->getMessage(), "\n";
    }
}
run();
--EXPECT--
{"x":[1,2]}
lit/2.5 abab a,b
local through
local!
got in one
destruct one
one
in two
destruct two
in three
destruct three
sum twothree
in four
destruct four
four
destruct error first
first
destruct error second
8
3
2020-02-03
end of round 0
{"x":[1,2]}
lit/2.5 abab a,b
local through
local!
got in one
destruct one
one
in two
destruct two
in three
destruct three
sum twothree
in four
destruct four
four
destruct error first
first
destruct error second
8
3
2020-02-03
end of round 1
Error: byRef(): Argument #1 ($x) could not be passed by reference
