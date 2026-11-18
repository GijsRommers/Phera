<?php
$d=new SQLite3(':memory:');
$r=$d->query("SELECT X'00FF',X'',CAST(X'FF00' AS TEXT)");
var_dump($r->fetchArray(SQLITE3_NUM));
