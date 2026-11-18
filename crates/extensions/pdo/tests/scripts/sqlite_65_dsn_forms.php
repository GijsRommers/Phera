<?php
foreach (['sqlite:', 'sqlite:relative.db',
'sqlite:file:uri.db?mode=rwc'] as $dsn) {
    $p=new PDO($dsn,null,null,[Pdo\Sqlite::ATTR_OPEN_FLAGS=>6|64]);
    $p->exec('CREATE TABLE a(x);INSERT INTO a VALUES(42)');
    var_dump($p->query('SELECT x FROM a')->fetchColumn());
}
