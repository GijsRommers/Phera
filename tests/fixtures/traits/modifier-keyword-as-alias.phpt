--TEST--
A keyword can name a trait alias after its modifier
--FILE--
<?php
trait T { public function sample() { return 'trait'; } }
class C {
    use T { sample as protected final; }
    public function both() { return [$this->sample(), $this->final()]; }
}
var_dump((new C)->both());
--EXPECT--
array(2) {
  [0]=>
  string(5) "trait"
  [1]=>
  string(5) "trait"
}
