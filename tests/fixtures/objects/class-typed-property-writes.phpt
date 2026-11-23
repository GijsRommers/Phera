--TEST--
Objects written into class-typed properties, often enough for the loop to take them: the type named in another case than the class, a subclass, an interface, a union, a nullable type, self and parent, an intersection, and objects the type refuses.
--FILE--
<?php
// Writes of objects into class-typed properties, often enough for the loop to
// take them: the type named in another case than the class, a subclass, an
// interface, a union, a nullable type, self and parent, an intersection, and
// objects the type refuses.

interface Named {}
interface Tagged {}
class Connection {}
class PdoConnection extends Connection implements Named, Tagged {}
class Other implements Named {}

class Holder {
    public connection $lower;
    public Connection $exact;
    public ?CONNECTION $nullable = null;
    public Named $named;
    public Connection|Other $union;
    public Named&Tagged $both;
    public ?self $me = null;
    public function set($value) {
        $this->lower = $value;
        $this->exact = $value;
        $this->nullable = $value;
        $this->union = $value;
    }
}
class Sub extends Holder { public ?parent $up = null; }

$h = new Sub;
for ($i = 0; $i < 20; $i++) {
    $h->set($i % 2 ? new Connection : new PdoConnection);
    $h->named = $i % 2 ? new Other : new PdoConnection;
    $h->both = new PdoConnection;
    $h->me = $i % 3 ? new Holder : null;
    $h->up = new Sub;
}
echo get_class($h->lower), ' ', get_class($h->exact), ' ', get_class($h->nullable), ' ', get_class($h->named), ' ', get_class($h->union), ' ', get_class($h->both), ' ', var_export($h->me, true), ' ', get_class($h->up), "\n";
foreach (['lower', 'exact', 'named', 'both', 'me'] as $property) {
    try { $h->$property = new stdClass; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
}
try { $h->both = new Other; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $h->set(new Other); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $h->up = new Connection; } catch (TypeError $e) { echo $e->getMessage(), "\n"; }

--EXPECTF--
Connection Connection Connection Other Connection PdoConnection \Holder::__set_state(array(
   'nullable' => NULL,
   'me' => NULL,
)) Sub
Cannot assign stdClass to property Holder::$lower of type connection
Cannot assign stdClass to property Holder::$exact of type Connection
Cannot assign stdClass to property Holder::$named of type Named
Cannot assign stdClass to property Holder::$both of type Named&Tagged
Cannot assign stdClass to property Holder::$me of type ?Holder
Cannot assign Other to property Holder::$both of type Named&Tagged
Cannot assign Other to property Holder::$lower of type connection
Cannot assign Connection to property Sub::$up of type ?Holder
