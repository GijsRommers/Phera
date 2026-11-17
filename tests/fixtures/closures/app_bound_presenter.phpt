--TEST--
app bound presenter
--FILE--
<?php
class Record {
    public function __construct(private $name, private $score) {}
    public function format(callable $formatter) {
        return $formatter->call($this);
    }
}
$formatter = function () {
    if ($this->score < 0) {
        throw new RuntimeException('score');
    }
    return strtoupper($this->name) . ':' . $this->score;
};
$records = [new Record('ada', 8), new Record('bob', 5)];
$lines = array_map(fn($record) => $record->format($formatter), $records);
var_dump($lines);
$record = new Record('cy', 9);
$bound = Closure::bind($formatter, $record, Record::class);
echo $bound(), "\n";
$other = $bound->bindTo(new Record('dan', 4));
echo $other(), "\n";
echo $bound(), "\n";
try {
    $invalid = new Record('eve', -1);
    $invalid->format($formatter);
} catch (RuntimeException $error) {
    echo $error->getMessage(), "\n";
} finally {
    echo 'rendered:', count($lines), "\n";
}
$copy = clone $bound;
var_dump($copy === $bound, $copy());
--EXPECT--
array(2) {
  [0]=>
  string(5) "ADA:8"
  [1]=>
  string(5) "BOB:5"
}
CY:9
DAN:4
CY:9
score
rendered:2
bool(false)
string(4) "CY:9"
