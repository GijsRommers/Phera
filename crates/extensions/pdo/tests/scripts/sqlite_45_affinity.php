<?php
$p=new PDO('sqlite::memory:');
$p->exec('CREATE TABLE a(i INTEGER, r REAL, t TEXT, b BLOB, n NUMERIC, u)');
foreach ([null, true, false, 0, 42, -1,
PHP_INT_MAX, PHP_INT_MIN, 1.25, "42",
"1.25", "", "abc", "a\0b", "\xff"] as $v) {
    foreach ([PDO::PARAM_STR, PDO::PARAM_INT,
    PDO::PARAM_LOB, PDO::PARAM_NULL] as $type) {
        $s=$p->prepare('INSERT INTO a VALUES(?,?,?,?,?,?)');
        for($i=1;
        $i<=6;
        $i++)$s->bindValue($i,$v,$type);
        $s->execute();
        var_dump($p->query('SELECT ' .
            '*,typeof(i),typeof(r),typeof(t),typeof(b),typeof(n),typeof(u) ' .
            'FROM a ORDER BY rowid DESC LIMIT 1')
            ->fetch(PDO::FETCH_NUM));
    }
}
