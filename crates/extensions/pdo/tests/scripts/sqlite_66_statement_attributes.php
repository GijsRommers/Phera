<?php
$p=PDO::connect('sqlite::memory:');
foreach(['SELECT 1 UNION ALL SELECT 2',
'EXPLAIN SELECT 1','EXPLAIN QUERY PLAN SELECT 1'] as $sql) {
    $s=$p->prepare($sql);
    var_dump($s->getAttribute(Pdo\Sqlite::ATTR_READONLY_STATEMENT),
    $s->getAttribute(Pdo\Sqlite::ATTR_BUSY_STATEMENT),
    $s->getAttribute(Pdo\Sqlite::ATTR_EXPLAIN_STATEMENT));
    $s->execute();
    var_dump($s->getAttribute(Pdo\Sqlite::ATTR_BUSY_STATEMENT));
    $s->fetchAll();
    var_dump($s->getAttribute(Pdo\Sqlite::ATTR_BUSY_STATEMENT));
}
