<?php
// Rule: method.notFound, method.nonObject and property.nonObject on known expressions.
class KnownReceiver {
    public int $count = 0;
    public function run(): int { return $this->count; }
}
class OtherReceiver {}
/** @return list<KnownReceiver> */
function loadReceivers(): array { return [new KnownReceiver]; }
function badMembers(KnownReceiver $item, KnownReceiver|OtherReceiver $union): void {
    $union->run();
    $union->missing();
    $item->missing();
    echo $item->missing;
    loadReceivers()[0]->missing();
    (42)->missing();
    echo (42)->missing;
    $nothing = null;
    $nothing?->missing();
}
