--TEST--
Native classes: constant-property-default
--FILE--
<?php
class Holder {
 public $mode=PDO::FETCH_ASSOC;
 public array $modes=[PDO::FETCH_NUM, PDO::FETCH_BOTH];
 }
 $h=new Holder;
 var_dump($h->mode,$h->modes);

--EXPECT--
int(2)
array(2) {
  [0]=>
  int(3)
  [1]=>
  int(4)
}
