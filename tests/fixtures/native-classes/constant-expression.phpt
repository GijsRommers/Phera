--TEST--
Native classes: constant-expression
--FILE--
<?php
const MASK=PDO::FETCH_ASSOC | PDO::FETCH_NUM;
 class Holder {
 const VALUE=PDO::FETCH_ASSOC + 10;
 }
 var_dump(MASK,Holder::VALUE);

--EXPECT--
int(3)
int(12)
