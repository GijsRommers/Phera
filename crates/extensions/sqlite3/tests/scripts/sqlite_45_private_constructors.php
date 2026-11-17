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
