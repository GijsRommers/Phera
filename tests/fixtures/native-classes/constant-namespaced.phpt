--TEST--
Native classes: constant-namespaced
--FILE--
<?php
namespace App;
 use PDO as Driver;
 const MODE=Driver::FETCH_NUM;
 class Holder {
 const MODE=Driver::FETCH_ASSOC;
 }
 var_dump(MODE,Holder::MODE);

--EXPECT--
int(3)
int(2)
