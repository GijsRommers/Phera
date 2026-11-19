--TEST--
A setter can change the receiver of a suspended generator
--FILE--
<?php
class Source {
    public int $count = 2 {
        get => $this->count;
        set {
            echo "set count\n";
            $this->count = $value * 2;
        }
    }
    public function counts(): Generator {
        yield $this->count;
        yield $this->count;
    }
}
$source = new Source();
$stream = $source->counts();
var_dump($stream->current());
$source->count = 5;
$stream->next();
var_dump($stream->current());
--EXPECT--
int(2)
set count
int(10)
