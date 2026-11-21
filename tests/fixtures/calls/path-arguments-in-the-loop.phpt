--TEST--
Path arguments sent by value and by reference give the same results as the general path
--FILE--
<?php
class Holder
{
    public $name = 'holder';
    public array $map = ['k' => 'v', 'n' => ['m' => 5]];
    private $hidden = 'secret';
    public function __get($name) { echo "get $name\n"; return "magic $name"; }
    public function show(): string { return label($this->hidden) . ' ' . label($this->name); }
}
function label($value) { return "[$value]"; }
function twice(float $value) { return $value * 2; }
function append(?array &$list) { $list[] = 'added'; }
function swap(&$value) { $value = 'swapped'; }
function strict(int $value) { return $value; }

function run(): void
{
    $holder = new Holder;
    $rows = ['title' => 'T', 'count' => 3, 'nested' => ['deep' => 1.5]];
    for ($round = 0; $round < 2; $round++) {
        echo label($holder->name), label($holder->map['k']), label($holder->map['n']['m']), "\n";
        echo label($rows['title']), twice($rows['count']), twice($rows['nested']['deep']), "\n";
        echo label($holder->missing), "\n";
        echo @label($rows['nope']), "\n";
        echo $holder->show(), "\n";
        append($rows['list']);
        swap($holder->map['k']);
        swap($rows['title']);
        echo json_encode($rows), json_encode($holder->map), "\n";
        $rows['title'] = 'T';
        $holder->map['k'] = 'v';
        echo strict($rows['count']), ' ', strtoupper($rows['title']), ' ', count($holder->map), "\n";
        try {
            echo twice($rows['title']), "\n";
        } catch (TypeError $e) {
            echo get_class($e), "\n";
        }
        echo label($holder?->name), "\n";
    }
}
run();
--EXPECT--
[holder][v][5]
[T]63
get missing
[magic missing]
[]
[secret] [holder]
{"title":"swapped","count":3,"nested":{"deep":1.5},"list":["added"]}{"k":"swapped","n":{"m":5}}
3 T 2
TypeError
[holder]
[holder][v][5]
[T]63
get missing
[magic missing]
[]
[secret] [holder]
{"title":"swapped","count":3,"nested":{"deep":1.5},"list":["added","added"]}{"k":"swapped","n":{"m":5}}
3 T 2
TypeError
[holder]
