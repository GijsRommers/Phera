<?php
// Rule: known receiver types through PHPDoc, array offsets and guarded member calls.
class ReadyReceiver {
    public int $count = 0;
    public function run(): int { return $this->count; }
}
class EmptyReceiver {}
class ReceiverSource {
    /** @return array{item: ReadyReceiver} */
    public function items(): array { return ['item' => new ReadyReceiver]; }
    public function get(): object { return new ReadyReceiver; }
}
function goodMembers(ReceiverSource $source, ReadyReceiver|EmptyReceiver $union): int {
    $union->run();
    $source->items()['item']->run();
    if ($source->get() instanceof ReadyReceiver) {
        $source->get()->run();
    }
    /** @var ReadyReceiver $item */
    $item = $source->get();
    return $item->count;
}
