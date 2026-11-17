--TEST--
app unit of work
--FILE--
<?php
class UnitOfWork {
    private $pending = [];
    public function enqueue(callable $operation) {
        $this->pending[] = $operation;
    }
    public function commit(&$log) {
        try {
            foreach ($this->pending as $operation) {
                $operation();
            }
        } catch (RuntimeException $error) {
            $log[] = 'rollback:' . $error->getMessage();
            throw $error;
        } finally {
            $this->pending = [];
            $log[] = 'clear';
        }
    }
}
$work = new UnitOfWork;
$log = [];
$work->enqueue(function () use (&$log) {
    $log[] = strtoupper('first');
});
$work->enqueue(function () {
    throw new RuntimeException('save');
});
try {
    $work->commit($log);
} catch (RuntimeException $error) {
    echo $error->getMessage(), "\n";
}
$work->commit($log);
var_dump($log);
--EXPECT--
save
array(4) {
  [0]=>
  string(5) "FIRST"
  [1]=>
  string(13) "rollback:save"
  [2]=>
  string(5) "clear"
  [3]=>
  string(5) "clear"
}
