--TEST--
app retry finally
--FILE--
<?php
class Retry {
    public function run(callable $operation, $attempts, &$log) {
        for ($attempt = 1; $attempt <= $attempts; ++$attempt) {
            try {
                return $operation($attempt);
            } catch (RuntimeException $error) {
                $log[] = $error->getMessage();
                if ($attempt === $attempts) {
                    throw $error;
                }
            } finally {
                $log[] = 'finally:' . $attempt;
            }
        }
    }
}
$retry = new Retry;
$log = [];
$result = $retry->run(function ($attempt) {
    if ($attempt < 3) {
        throw new RuntimeException('try:' . $attempt);
    }
    return strtoupper('success');
}, 4, $log);
var_dump($result, $log);
try {
    $retry->run(function ($attempt) {
        throw new RuntimeException('fail:' . $attempt);
    }, 2, $log);
} catch (RuntimeException $error) {
    echo $error->getMessage(), "\n";
}
var_dump(count($log));
--EXPECT--
string(7) "SUCCESS"
array(5) {
  [0]=>
  string(5) "try:1"
  [1]=>
  string(9) "finally:1"
  [2]=>
  string(5) "try:2"
  [3]=>
  string(9) "finally:2"
  [4]=>
  string(9) "finally:3"
}
fail:2
int(9)
