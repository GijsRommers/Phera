<?php
$sql = <<<'SQL'
    SELECT n,
        CASE WHEN n NOT BETWEEN 65 AND 128 AND n % 2 = 0
            THEN '2026-11-17 12:34:56.123456+00'::timestamptz END AS moment,
        CASE WHEN n NOT BETWEEN 65 AND 128 AND n % 3 = 0
            THEN '1 year -2 months 3 days 04:05:06.12'::interval END AS span,
        CASE WHEN n NOT BETWEEN 65 AND 128 AND n % 5 = 0
            THEN ARRAY[n, NULL, n + 1] END AS numbers,
        CASE WHEN n NOT BETWEEN 65 AND 128 AND n % 7 = 0
            THEN '192.0.2.1'::inet END AS address,
        1.2500::numeric AS decimal
    FROM generate_series(1, 130) AS series(n)
    ORDER BY n
    SQL;

foreach ([[true, false], [false, false], [false, true]] as $mode => [$prefetch, $unnamed]) {
    $pdo = new PDO('pgsql:host=127.0.0.1;port=54329;dbname=phera_test', 'phera', 'phera', [
        PDO::ATTR_PREFETCH => $prefetch,
        Pdo\Pgsql::ATTR_DISABLE_PREPARES => $unnamed,
    ]);
    [$date, $zone, $interval] = [
        ['ISO, MDY', 'UTC', 'postgres'],
        ['SQL, DMY', 'Asia/Kathmandu', 'iso_8601'],
        ['Postgres, MDY', 'America/New_York', 'postgres'],
    ][$mode];
    $pdo->exec("SET DateStyle TO '$date'; SET TIME ZONE '$zone'; SET IntervalStyle TO '$interval'");
    $statement = $pdo->prepare($sql);
    $statement->execute();
    $first = $statement->fetch(PDO::FETCH_ASSOC);
    $remaining = $statement->fetchAll(PDO::FETCH_ASSOC);
    var_dump($first, count($remaining), $remaining[62], $remaining[63], $remaining[127]);
    echo hash('sha256', serialize($remaining)), "\n";
}

$pdo->beginTransaction();
$pdo->exec("DECLARE page_cursor SCROLL CURSOR FOR $sql");
var_dump($pdo->query('FETCH ABSOLUTE 129 FROM page_cursor')->fetch(PDO::FETCH_ASSOC));
var_dump($pdo->query('FETCH BACKWARD 2 FROM page_cursor')->fetchAll(PDO::FETCH_ASSOC));
$pdo->rollBack();
