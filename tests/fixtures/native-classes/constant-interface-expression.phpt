--TEST--
Native classes: constant-interface-expression
--FILE--
<?php
interface Modes {
 const MODE=PDO::FETCH_ASSOC;
 }
 class Holder implements Modes {
}
 var_dump(Modes::MODE,Holder::MODE);

--EXPECT--
int(2)
int(2)
