<?php
// Phera deliberately differs from the PHP 8.5 PDO scanner for these templates.
$pdo = new PDO('pgsql:host=127.0.0.1;port=54329;dbname=phera_test', 'phera', 'phera');
$pdo->setAttribute(PDO::ATTR_EMULATE_PREPARES, true);
$comment = $pdo->prepare('SELECT 1 /* a /* ? */ :v */');
$comment->execute();
echo 'nested comment ', $comment->fetchColumn(), "\n";
foreach ([
    ['comment binding', $comment, [':v' => 'unused']],
    ['unterminated literal', $pdo->prepare("SELECT 'abc ? AS y"), ['p']],
] as [$label, $statement, $values]) {
    try {
        $statement->execute($values);
        echo $label, " accepted\n";
    } catch (PDOException $error) {
        echo $label, ' ', $error->getCode(), "\n";
    }
}
