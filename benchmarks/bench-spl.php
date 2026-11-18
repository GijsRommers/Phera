<?php
// Give a case name to measure that workload in isolation.
function array_container(string $class, int $size): int
{
    $container = new $class();
    for ($i = 0; $i < $size; ++$i) {
        $container[] = $i;
    }
    $sum = 0;
    foreach ($container as $value) {
        $sum += $value;
    }
    for ($i = 0; $i < $size; ++$i) {
        $sum += $container[$i];
    }
    return $sum;
}

function stack(int $size): int
{
    $stack = new SplStack();
    for ($i = 0; $i < $size; ++$i) {
        $stack->push($i);
    }
    $sum = 0;
    for ($i = 0; $i < $size; ++$i) {
        $sum += $stack->pop();
    }
    return $sum;
}

function queue(int $size): int
{
    $queue = new SplQueue();
    for ($i = 0; $i < $size; ++$i) {
        $queue->enqueue($i);
    }
    $sum = 0;
    for ($i = 0; $i < $size; ++$i) {
        $sum += $queue->dequeue();
    }
    return $sum;
}

function object_storage(int $size): int
{
    $storage = new SplObjectStorage();
    for ($i = 0; $i < $size; ++$i) {
        $storage[new stdClass()] = $i;
    }
    $sum = 0;
    foreach ($storage as $object) {
        $sum += $storage[$object];
    }
    return $sum;
}

function priority_queue(int $size): int
{
    $queue = new SplPriorityQueue();
    for ($i = 0; $i < $size; ++$i) {
        $queue->insert($i, ($i * 7919) % $size);
    }
    $sum = 0;
    for ($i = 0; $i < $size; ++$i) {
        $sum += $queue->extract();
    }
    return $sum;
}

$cases = [
    'array-object' => static fn() => array_container(ArrayObject::class, 1000000),
    'array-iterator' => static fn() => array_container(ArrayIterator::class, 1000000),
    'stack' => static fn() => stack(1000000),
    'queue' => static fn() => queue(1000000),
    'object-storage' => static fn() => object_storage(1000000),
    'priority-queue' => static fn() => priority_queue(100000),
];
foreach ($cases as $case => $run) {
    if (!isset($argv[1]) || $argv[1] === $case) {
        echo $case, ' ', $run(), PHP_EOL;
    }
}
