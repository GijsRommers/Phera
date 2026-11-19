--TEST--
Asymmetric properties allow reads and reject writes outside their set scope
--FILE--
<?php
class Account {
    private(set) static int $next = 12;
    public function __construct(private(set) int $id, public protected(set) array $items = []) {}
}
class Customer extends Account {
    public function addItem(string $item): void { $this->items[] = $item; }
    public function changeId(int $id): void { $this->id = $id; }
}
$client = new Customer(7);
$client->addItem('apple');
var_dump($client->id, $client->items, Account::$next);
foreach (['id', 'items', 'next'] as $name) {
    $p = new ReflectionProperty(Account::class, $name);
    var_dump($p->isPublic(), $p->isPrivateSet(), $p->isProtectedSet(), $p->isFinal());
}
try { $client->changeId(8); } catch (Error $error) { echo $error->getMessage(), "\n"; }
try { $client->items[] = 'orange'; } catch (Error $error) { echo $error->getMessage(), "\n"; }
try { Account::$next = 5; } catch (Error $error) { echo $error->getMessage(), "\n"; }
(new ReflectionProperty(Account::class, 'id'))->setValue($client, '9');
var_dump($client->id);
--EXPECT--
int(7)
array(1) {
  [0]=>
  string(5) "apple"
}
int(12)
bool(true)
bool(true)
bool(false)
bool(true)
bool(true)
bool(false)
bool(true)
bool(false)
bool(true)
bool(true)
bool(false)
bool(true)
Cannot modify private(set) property Account::$id from scope Customer
Cannot indirectly modify protected(set) property Account::$items from global scope
Cannot modify private(set) property Account::$next from global scope
int(9)
