--TEST--
transaction-guard
--FILE--
<?php
class Connection { public array $log = []; }
class Transaction {
    private bool $committed = false;
    public function __construct(private Connection $connection) { $connection->log[] = "begin"; }
    public function commit(): void { $this->committed = true; $this->connection->log[] = "commit"; }
    public function __destruct() { if (!$this->committed) { $this->connection->log[] = "rollback"; } }
}
function transfer(Connection $c, int $amount): void {
    $tx = new Transaction($c);
    if ($amount > 100) { throw new RuntimeException("insufficient funds"); }
    $tx->commit();
}
$db = new Connection;
transfer($db, 50);
try { transfer($db, 500); } catch (RuntimeException $e) { $db->log[] = "caught: " . $e->getMessage(); }
echo implode("\n", $db->log), "\n";
--EXPECT--
begin
commit
begin
rollback
caught: insufficient funds
