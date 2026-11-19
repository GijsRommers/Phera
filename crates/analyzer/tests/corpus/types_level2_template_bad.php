<?php
// Rule: a template argument contributes exactly the members of the inferred class.
class Payload { public function run(): void {} }
/** @template T of object */
class PayloadBox {
    /** @return T */
    public function read(): object { return new stdClass; }
}
/** @extends PayloadBox<Payload> */
class Payloads extends PayloadBox {}
/**
 * @template Item of object
 * @param Item $item
 * @return Item
 */
function identityObject(object $item): object { return $item; }
(new Payloads)->read()->missing();
identityObject(new Payload)->missing();
