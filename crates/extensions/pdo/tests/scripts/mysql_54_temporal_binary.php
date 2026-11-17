<?php
$p = new PDO('mysql:host=127.0.0.1;port=33069;dbname=phera_test', 'phera', 'phera');
$p->exec('CREATE TEMPORARY TABLE temporal_binary (' .
    'd DATETIME(3), z DATETIME(3), t TIME(2), b BIT(8), wide BIT(64), raw VARBINARY(4))');
$p->exec("INSERT INTO temporal_binary VALUES ('2026-01-02 03:04:05.123'," .
    "'2026-01-02 03:04:05.000','-24:05:06.12',b'10101010'," .
    "b'1111111111111111111111111111111111111111111111111111111111111111',X'00FF7F')");
foreach ([false, true] as $emulate) {
    $p->setAttribute(PDO::ATTR_EMULATE_PREPARES, $emulate);
    var_dump($p->query('SELECT * FROM temporal_binary')->fetch(PDO::FETCH_NUM));
}
