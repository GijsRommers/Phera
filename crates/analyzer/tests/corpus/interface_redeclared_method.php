<?php
// Rule: arguments.count; an interface that redeclares its parent's method with more optional parameters is checked against its own declaration.
interface ObjectManager { public function find(string $class, mixed $id): ?object; public function refresh(object $o): void; }
interface EntityManager extends ObjectManager { public function find(string $class, mixed $id, ?int $lock = null, ?int $version = null): ?object; public function refresh(object $o, ?int $lock = null): void; }
class Service { public function __construct(private EntityManager $em) {} public function run(object $o) { $this->em->refresh($o, 1); return $this->em->find('X', 1, 2); } }
function direct(EntityManager $em, object $o) { $em->refresh($o, 1); return $em->find('X', 1, 2, 3); }
abstract class Repo { public function findOneBy(array $c) {} } class EntityRepo extends Repo { public function findOneBy(array $c, ?array $order = null) {} }
function repo(EntityRepo $r) { return $r->findOneBy([], []); }
