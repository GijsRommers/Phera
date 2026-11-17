<?php
// PHP 8.5.12 leaves native $n tokens outside PDO's parameter map. Pure $n
// bindings send NULL; mixed PDO tokens rewrite to $1 and can share that slot.
foreach ([false, true] as $emulate) {
    foreach ([
        ['single', 'SELECT $1::int + 1', [41]],
        ['repeated', 'SELECT $1::int + $1::int', [21]],
        ['out of order', 'SELECT $2::int - $1::int', [2, 44]],
        ['gap', 'SELECT $2::int', [2, 44]],
        ['question', 'SELECT $1::int + ?::int', [21]],
        ['question extra', 'SELECT $1::int + ?::int', [21, 22]],
        ['named', 'SELECT $1::int + :name::int', ['name' => 21]],
        ['named numeric binding', 'SELECT $1::int + :name::int', [21]],
        ['missing', 'SELECT $1::int', []],
        ['sparse binding', 'SELECT $1::int', [5 => 41]],
        ['zero', 'SELECT $0::int', [41]],
        ['dollar name', 'SELECT $1::int', ['$1' => 41]],
        ['quoted', "SELECT '$1' AS a, $$\$2$$ AS b, ?::int AS c", [41]],
        ['comment', 'SELECT ?::int /* $2 */', [41]],
    ] as [$label, $sql, $parameters]) {
        $pdo = new PDO('pgsql:host=127.0.0.1;port=54329;dbname=phera_test', 'phera', 'phera');
        $pdo->setAttribute(PDO::ATTR_EMULATE_PREPARES, $emulate);
        echo $emulate ? "emulated " : "native ", $label, "\n";
        try {
            $statement = $pdo->prepare($sql);
            var_dump($statement->execute($parameters));
            var_dump($statement->fetch(PDO::FETCH_NUM));
        } catch (PDOException $error) {
            echo $error->getMessage(), "\n";
        }
    }
    foreach ([1, '$1'] as $parameter) {
        $pdo = new PDO('pgsql:host=127.0.0.1;port=54329;dbname=phera_test', 'phera', 'phera');
        $pdo->setAttribute(PDO::ATTR_EMULATE_PREPARES, $emulate);
        echo $emulate ? "emulated bind " : "native bind ", $parameter, "\n";
        try {
            $statement = $pdo->prepare('SELECT $1::int + 1');
            var_dump($statement->bindValue($parameter, 41, PDO::PARAM_INT));
            var_dump($statement->execute());
            var_dump($statement->fetchColumn());
        } catch (PDOException $error) {
            echo $error->getMessage(), "\n";
        }
    }
}
