--TEST--
Array casts and dumps use storage while object vars and JSON use getters
--FILE--
<?php
class C {
    public int $backed = 2 { get => $this->backed + 1; }
    public int $virtual { get => 7; }
    public int $onlySet { set {} }
    public int $uninitialized { set => $value; }
}
$c = new C;
var_dump($c, (array) $c, get_object_vars($c));
echo json_encode($c), "\n";
--EXPECT--
object(C)#1 (1) {
  ["backed"]=>
  int(2)
  ["uninitialized"]=>
  uninitialized(int)
}
array(1) {
  ["backed"]=>
  int(2)
}
array(3) {
  ["backed"]=>
  int(3)
  ["virtual"]=>
  int(7)
  ["uninitialized"]=>
  NULL
}
{"backed":3,"virtual":7,"uninitialized":null}
