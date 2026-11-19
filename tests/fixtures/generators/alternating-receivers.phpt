--TEST--
Alternating generator frames read the current property of their own receiver
--FILE--
<?php
class CounterSource {
    public function __construct(private int $value) {}

    public function add(int $amount): void {
        $this->value += $amount;
    }

    public function values(): Generator {
        for ($i = 0; $i < 3; ++$i) {
            yield $this->value;
        }
    }
}

$first = new CounterSource(10);
$second = new CounterSource(100);
$a = $first->values();
$b = $second->values();

echo $a->current(), "\n";
echo $b->current(), "\n";
$first->add(1);
$second->add(2);

$a->next();
echo $a->current(), "\n";
$b->next();
echo $b->current(), "\n";
$first->add(10);
$second->add(20);

$a->next();
echo $a->current(), "\n";
$b->next();
echo $b->current(), "\n";
--EXPECT--
10
100
11
102
21
122
