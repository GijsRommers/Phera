<?php
// PHP 8.5 oracle: 36 app repository.
$dsn='pgsql:host=127.0.0.1;port=54329;dbname=phera_test';
$p=new PDO($dsn,'phera','phera');
$t='phera_pg_36_app_repository';
$p->exec("DROP TABLE IF EXISTS $t");
$p->exec("CREATE TABLE $t (id SERIAL PRIMARY KEY, label VARCHAR(80), score INT,
    amount DECIMAL(12,2), active BOOLEAN)");
try {
    $p->exec("INSERT INTO $t (label,score,amount,active) VALUES
        ('A',10,1.25,true),('B',20,2.50,false),('C',30,3.75,true)");
    class UserRepository {
        public function __construct(private PDO $db, private string $table) {}
        public function find(int $id): array|false {
            $s=$this->db->prepare("SELECT id,label FROM ".$this->table." WHERE id=:id");
            $s->execute(['id'=>$id]); return $s->fetch(PDO::FETCH_ASSOC);
        }
        public function rename(int $id,string $name): bool {
            $s=$this->db->prepare("UPDATE ".$this->table." SET label=? WHERE id=?");
            return $s->execute([$name,$id]);
        }
    }
    $r=new UserRepository($p,$t); var_dump($r->rename(2,'Renamed'),$r->find(2),$r->find(999));
 } finally {
    if ($p->inTransaction()) {$p->rollBack();}
    $p->exec("DROP TABLE IF EXISTS $t");
}
