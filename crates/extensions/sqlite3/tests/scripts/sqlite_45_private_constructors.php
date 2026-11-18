<?php
try {
    new SQLite3Stmt();
}
catch(Throwable $e){
    var_dump(get_class($e),$e->getMessage());
}
try {
    new SQLite3Result();
}
catch(Throwable $e){
    var_dump(get_class($e),$e->getMessage());
}
var_dump(SQLite3Stmt::EXPLAIN_MODE_PREPARED,
SQLite3Stmt::EXPLAIN_MODE_EXPLAIN, SQLite3Stmt::EXPLAIN_MODE_EXPLAIN_QUERY_PLAN);

$d = new SQLite3(':memory:');
$s = $d->prepare('SELECT 42');
$r = $s->execute();
foreach ([$s, $r] as $object) {
    try {$object->__construct();}
    catch (Throwable $e) {var_dump(get_class($e), $e->getMessage());}
}
