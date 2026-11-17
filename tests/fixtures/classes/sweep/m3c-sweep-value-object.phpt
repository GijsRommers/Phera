--TEST--
Application class sweep: value-object
--FILE--
<?php
class Money {
 public int $amount;
 private function __construct(
int $amount)
 {
$this->amount=$amount;
}
 static function fromArray(
array $data)
: static {
return new static(
$data['amount'])
;
}
 function plus(
Money $other)
: Money {
return new Money(
$this->amount+$other->amount);
}
 }
 $first=Money::fromArray(['amount'=>12]);
 $second=Money::fromArray(['amount'=>8]);
 $sum=$first->plus($second);
 echo $first->amount,
"/",
$second->amount,
"/",
$sum->amount,
"\n";

--EXPECT--
12/8/20
