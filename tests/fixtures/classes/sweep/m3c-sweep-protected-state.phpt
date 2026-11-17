--TEST--
Application class sweep: protected-state
--FILE--
<?php
class Counter {
protected int $value=0;
function add(
int $amount)
: static {
$this->value+=$amount;
return $this;
}
function value(
)
: int {
return $this->value;
}
}
class DoubleCounter extends Counter {
function add(
int $amount)
: static {
return parent::add(
$amount*2);
}
}
$counter=new DoubleCounter;
$counter->add(2)->add(3);
echo $counter->value(),
"\n";
$base=new Counter;
$base->add(5);
echo $base->value(),
"\n";

--EXPECT--
10
5
