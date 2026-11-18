--TEST--
Native classes: constant-other-class
--FILE--
<?php
class Holder {
 const MODE=PDO::FETCH_ASSOC;
 const OPTIONS=[PDO::ATTR_ERRMODE=>PDO::ERRMODE_EXCEPTION];
 }
 var_dump(Holder::MODE,Holder::OPTIONS);

--EXPECT--
int(2)
array(1) {
  [3]=>
  int(2)
}
