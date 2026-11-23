--TEST--
Appends to arrays in variables and properties give the same results as the general path
--FILE--
<?php
class Bag
{
    public array $items = [];
    public array $groups = ['a' => []];
    public ?array $maybe = null;
    public readonly array $fixed;
    public function __construct() { $this->fixed = []; }
    public function add($value): void { $this->items[] = $value; $this->groups['a'][] = $value; }
}
function run(): void
{
    $bag = new Bag;
    $alias = $bag->items;
    for ($round = 0; $round < 3; $round++) {
        $bag->add($round);
        $bag->items[] = [$round];
        $lists = ['x' => [1], 'big' => [PHP_INT_MAX => 'last']];
        $copy = $lists;
        $lists['x'][] = $round;
        $kept = $lists['x'][] = 'k';
        try {
            $lists['big'][] = 'overflow';
        } catch (Error $e) {
            echo get_class($e), ': ', $e->getMessage(), "\n";
        }
        $lists['missing'][] = 1;
        try {
            $bag->fixed[] = 1;
        } catch (Error $e) {
            echo $e->getMessage(), "\n";
        }
        $bag->maybe[] = $round;
        echo json_encode($bag->items), json_encode($bag->groups), json_encode($bag->maybe), json_encode($alias), "\n";
        echo json_encode($lists), json_encode($copy), $kept, "\n";
    }
}
run();
--EXPECT--
Error: Cannot add element to the array as the next element is already occupied
Cannot indirectly modify readonly property Bag::$fixed
[0,[0]]{"a":[0]}[0][]
{"x":[1,0,"k"],"big":{"9223372036854775807":"last"},"missing":[1]}{"x":[1],"big":{"9223372036854775807":"last"}}k
Error: Cannot add element to the array as the next element is already occupied
Cannot indirectly modify readonly property Bag::$fixed
[0,[0],1,[1]]{"a":[0,1]}[0,1][]
{"x":[1,1,"k"],"big":{"9223372036854775807":"last"},"missing":[1]}{"x":[1],"big":{"9223372036854775807":"last"}}k
Error: Cannot add element to the array as the next element is already occupied
Cannot indirectly modify readonly property Bag::$fixed
[0,[0],1,[1],2,[2]]{"a":[0,1,2]}[0,1,2][]
{"x":[1,2,"k"],"big":{"9223372036854775807":"last"},"missing":[1]}{"x":[1],"big":{"9223372036854775807":"last"}}k
