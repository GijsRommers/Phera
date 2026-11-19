--TEST--
A shared delegate can move to a different leaf while an outer generator is suspended
--FILE--
<?php
function leaf() {
    $message = yield 'leaf';
    yield $message;
    return 'done';
}
function switchSource() {
    yield 'first';
    return yield from leaf();
}
function relay($source) {
    return yield from $source;
}

$shared = switchSource();
$left = relay(relay($shared));
$right = relay($shared);
echo $left->current(), "\n";
echo $right->current(), "\n";
$right->next();
echo $left->current(), "\n";
echo $left->send('received'), "\n";
echo $right->current(), "\n";
$right->next();
echo $right->getReturn(), "\n";
$left->next();
echo $left->getReturn(), "\n";
--EXPECT--
first
first
leaf
received
received
done
done
