<?php
$d=new SQLite3(':memory:');
var_dump($d->querySingle('SELECT ' .
    '-9223372036854775808,9223372036854775807,1.2345678901234567,1e300,1e-300',
true));
