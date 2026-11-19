--TEST--
List foreach uses separate cursors and FIFO delete mode drains a queue
--FILE--
<?php
$queue = new SplQueue;
$queue->enqueue(1);
$queue->enqueue(2);
foreach ($queue as $key => $value) {
    echo $key, ':', $value, ':', $queue->valid() ? 'valid' : 'invalid', "\n";
    if ($key === 0) {
        foreach ($queue as $inner => $nested) {
            echo 'inner ', $inner, ':', $nested, "\n";
        }
    }
}
$queue->setIteratorMode(SplQueue::IT_MODE_FIFO | SplQueue::IT_MODE_DELETE);
foreach ($queue as $key => $value) {
    echo 'remove ', $key, ':', $value, "\n";
}
var_dump($queue->isEmpty());
--EXPECT--
0:1:invalid
inner 0:1
inner 1:2
1:2:invalid
remove 0:1
remove 0:2
bool(true)
