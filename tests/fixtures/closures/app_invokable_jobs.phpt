--TEST--
app invokable jobs
--FILE--
<?php
class Job {
    public function __construct(private $prefix) {}
    public function __invoke($name) {
        if ($name === '') {
            throw new RuntimeException('empty job');
        }
        return $this->prefix . strtoupper($name);
    }
    public function callback() {
        return $this(...);
    }
}
class Queue {
    public function execute(callable $job, $names) {
        $results = [];
        foreach ($names as $name) {
            try {
                $results[] = $job($name);
            } catch (RuntimeException $error) {
                $results[] = $error->getMessage();
            }
        }
        return $results;
    }
}
$job = new Job('job:');
$queue = new Queue;
var_dump($queue->execute($job, ['ada', '', 'bob']));
$closure = Closure::fromCallable($job);
var_dump(array_map($closure, ['cy', 'dan']));
$callback = $job->callback();
var_dump($callback instanceof Closure, call_user_func($callback, 'eve'));
--EXPECT--
array(3) {
  [0]=>
  string(7) "job:ADA"
  [1]=>
  string(9) "empty job"
  [2]=>
  string(7) "job:BOB"
}
array(2) {
  [0]=>
  string(6) "job:CY"
  [1]=>
  string(7) "job:DAN"
}
bool(true)
string(7) "job:EVE"
