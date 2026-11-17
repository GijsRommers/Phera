--TEST--
app middleware pipeline
--FILE--
<?php
class Pipeline {
    private $stages = [];
    public function add(callable $stage) {
        $this->stages[] = $stage;
    }
    public function handle($request) {
        $next = fn($request) => strtoupper($request['name']);
        foreach (array_reverse($this->stages) as $stage) {
            $downstream = $next;
            $next = function ($request) use ($stage, $downstream) {
                return $stage($request, $downstream);
            };
        }
        return $next($request);
    }
}
$pipeline = new Pipeline;
$log = [];
$pipeline->add(function ($request, $next) use (&$log) {
    $log[] = 'enter';
    try {
        return '[' . $next($request) . ']';
    } finally {
        $log[] = 'leave';
    }
});
$pipeline->add(function ($request, $next) {
    if ($request['name'] === '') {
        throw new RuntimeException('name required');
    }
    return $next($request);
});
echo $pipeline->handle(['name' => 'Ada']), "\n";
try {
    $pipeline->handle(['name' => '']);
} catch (RuntimeException $error) {
    echo $error->getMessage(), "\n";
}
var_dump($log);
--EXPECT--
[ADA]
name required
array(4) {
  [0]=>
  string(5) "enter"
  [1]=>
  string(5) "leave"
  [2]=>
  string(5) "enter"
  [3]=>
  string(5) "leave"
}
