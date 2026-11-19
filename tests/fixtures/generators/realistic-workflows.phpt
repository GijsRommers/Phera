--TEST--
A lazy pipeline, keyed data provider and send-driven scheduler
--FILE--
<?php
function read_lines($source): Generator {
    while (($line = fgets($source)) !== false) {
        yield rtrim($line, "\n");
    }
}
function active(iterable $lines): Generator {
    foreach ($lines as $line) {
        if (str_starts_with($line, '+')) {
            yield substr($line, 1);
        }
    }
}
function labels(iterable $lines): Generator {
    foreach ($lines as $line) {
        yield strtoupper($line);
    }
}
function pipeline($input): Generator {
    yield from labels(active(read_lines($input)));
}
$input = fopen('php://memory', 'w+');
fwrite($input, "+one\n-two\n+three\n");
rewind($input);
foreach (pipeline($input) as $line) {
    echo "$line\n";
}
fclose($input);

function data_provider(): Generator {
    yield 'empty' => ['', 0];
    yield 'text' => ['Phera', 5];
}
foreach (iterator_to_array(data_provider()) as $name => [$text, $length]) {
    echo "$name ", strlen($text) === $length ? 'ok' : 'failed', "\n";
}

function worker(string $name): Generator {
    $first = yield "$name waiting";
    $second = yield "$name:$first";
    return "$name:" . ($first + $second);
}
$workers = [worker('A'), worker('B'), worker('C')];
foreach ($workers as $worker) {
    echo $worker->current(), "\n";
}
foreach ($workers as $index => $worker) {
    echo $worker->send($index + 1), "\n";
}
foreach ($workers as $worker) {
    $worker->send(10);
    echo $worker->getReturn(), "\n";
}
--EXPECT--
ONE
THREE
empty ok
text ok
A waiting
B waiting
C waiting
A:1
B:2
C:3
A:11
B:12
C:13
