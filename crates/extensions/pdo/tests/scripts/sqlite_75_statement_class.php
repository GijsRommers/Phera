<?php
class TracedStatement extends PDOStatement {
    protected function __construct(public string $label) {
        echo $this->label, ': ', $this->queryString, "\n";
    }
}
class PublicStatement extends PDOStatement {
    public function __construct() {}
}
$pdo = new PDO('sqlite::memory:', options: [
    PDO::ATTR_STATEMENT_CLASS => [TracedStatement::class, ['trace']],
]);
var_dump($pdo->getAttribute(PDO::ATTR_STATEMENT_CLASS));
var_dump($pdo->query('SELECT 1')->fetchColumn());
$statement = $pdo->prepare('SELECT ?');
$statement->execute([7]);
var_dump($statement->fetchColumn());
try {
    $pdo->setAttribute(PDO::ATTR_STATEMENT_CLASS, [PublicStatement::class]);
} catch (TypeError $e) {
    echo $e->getMessage(), "\n";
}
var_dump($pdo->setAttribute(PDO::ATTR_STATEMENT_CLASS, [PDOStatement::class]));
var_dump(get_class($pdo->query('SELECT 1')));
