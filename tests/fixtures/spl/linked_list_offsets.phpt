--TEST--
Linked-list dimensions coerce keys and report invalid keys for each list mode
--FILE--
<?php
foreach ([SplDoublyLinkedList::class, SplStack::class, SplQueue::class] as $class) {
    $list = new $class;
    $list->push('a');
    $list->push('b');
    echo $class, "\n";
    foreach ([false, true, null, '1', '01', 'x', -1, 3, 1.5, [], new stdClass] as $key) {
        echo get_debug_type($key);
        foreach (['read','exists','write','unset'] as $operation) {
            $copy = clone $list;
            try {
                if ($operation === 'read') {
                    $result = $copy[$key];
                } elseif ($operation === 'exists') {
                    $result = isset($copy[$key]);
                } elseif ($operation === 'write') {
                    $copy[$key] = 'c';
                    $result = iterator_to_array($copy, false);
                } else {
                    unset($copy[$key]);
                    $result = iterator_to_array($copy, false);
                }
                echo ' ', json_encode($result);
            } catch (Throwable $error) {
                echo ' ', get_class($error), '/', $error->getMessage();
            }
        }
        echo "\n";
    }
}
--EXPECTF--
SplDoublyLinkedList
bool "a" true ["c","b"] ["b"]
bool "b" true ["a","c"] ["a"]
null
Deprecated: SplDoublyLinkedList::offsetGet(): Passing null to parameter #1 ($index) of type int is deprecated in %s on line 13
 "a"
Deprecated: SplDoublyLinkedList::offsetExists(): Passing null to parameter #1 ($index) of type int is deprecated in %s on line 15
 true ["a","b","c"]
Deprecated: SplDoublyLinkedList::offsetUnset(): Passing null to parameter #1 ($index) of type int is deprecated in %s on line 20
 ["b"]
string "b" true ["a","c"] ["a"]
string "b" true ["a","c"] ["a"]
string TypeError/SplDoublyLinkedList::offsetGet(): Argument #1 ($index) must be of type int, string given TypeError/SplDoublyLinkedList::offsetExists(): Argument #1 ($index) must be of type int, string given TypeError/SplDoublyLinkedList::offsetSet(): Argument #1 ($index) must be of type ?int, string given TypeError/SplDoublyLinkedList::offsetUnset(): Argument #1 ($index) must be of type int, string given
int OutOfRangeException/SplDoublyLinkedList::offsetGet(): Argument #1 ($index) is out of range false OutOfRangeException/SplDoublyLinkedList::offsetSet(): Argument #1 ($index) is out of range OutOfRangeException/SplDoublyLinkedList::offsetUnset(): Argument #1 ($index) is out of range
int OutOfRangeException/SplDoublyLinkedList::offsetGet(): Argument #1 ($index) is out of range false OutOfRangeException/SplDoublyLinkedList::offsetSet(): Argument #1 ($index) is out of range OutOfRangeException/SplDoublyLinkedList::offsetUnset(): Argument #1 ($index) is out of range
float
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 13
 "b"
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 15
 true
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 17
 ["a","c"]
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 20
 ["a"]
array TypeError/SplDoublyLinkedList::offsetGet(): Argument #1 ($index) must be of type int, array given TypeError/SplDoublyLinkedList::offsetExists(): Argument #1 ($index) must be of type int, array given TypeError/SplDoublyLinkedList::offsetSet(): Argument #1 ($index) must be of type ?int, array given TypeError/SplDoublyLinkedList::offsetUnset(): Argument #1 ($index) must be of type int, array given
stdClass TypeError/SplDoublyLinkedList::offsetGet(): Argument #1 ($index) must be of type int, stdClass given TypeError/SplDoublyLinkedList::offsetExists(): Argument #1 ($index) must be of type int, stdClass given TypeError/SplDoublyLinkedList::offsetSet(): Argument #1 ($index) must be of type ?int, stdClass given TypeError/SplDoublyLinkedList::offsetUnset(): Argument #1 ($index) must be of type int, stdClass given
SplStack
bool "b" true ["c","a"] ["a"]
bool "a" true ["b","c"] ["b"]
null
Deprecated: SplDoublyLinkedList::offsetGet(): Passing null to parameter #1 ($index) of type int is deprecated in %s on line 13
 "b"
Deprecated: SplDoublyLinkedList::offsetExists(): Passing null to parameter #1 ($index) of type int is deprecated in %s on line 15
 true ["c","b","a"]
Deprecated: SplDoublyLinkedList::offsetUnset(): Passing null to parameter #1 ($index) of type int is deprecated in %s on line 20
 ["a"]
string "a" true ["b","c"] ["b"]
string "a" true ["b","c"] ["b"]
string TypeError/SplDoublyLinkedList::offsetGet(): Argument #1 ($index) must be of type int, string given TypeError/SplDoublyLinkedList::offsetExists(): Argument #1 ($index) must be of type int, string given TypeError/SplDoublyLinkedList::offsetSet(): Argument #1 ($index) must be of type ?int, string given TypeError/SplDoublyLinkedList::offsetUnset(): Argument #1 ($index) must be of type int, string given
int OutOfRangeException/SplDoublyLinkedList::offsetGet(): Argument #1 ($index) is out of range false OutOfRangeException/SplDoublyLinkedList::offsetSet(): Argument #1 ($index) is out of range OutOfRangeException/SplDoublyLinkedList::offsetUnset(): Argument #1 ($index) is out of range
int OutOfRangeException/SplDoublyLinkedList::offsetGet(): Argument #1 ($index) is out of range false OutOfRangeException/SplDoublyLinkedList::offsetSet(): Argument #1 ($index) is out of range OutOfRangeException/SplDoublyLinkedList::offsetUnset(): Argument #1 ($index) is out of range
float
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 13
 "a"
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 15
 true
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 17
 ["b","c"]
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 20
 ["b"]
array TypeError/SplDoublyLinkedList::offsetGet(): Argument #1 ($index) must be of type int, array given TypeError/SplDoublyLinkedList::offsetExists(): Argument #1 ($index) must be of type int, array given TypeError/SplDoublyLinkedList::offsetSet(): Argument #1 ($index) must be of type ?int, array given TypeError/SplDoublyLinkedList::offsetUnset(): Argument #1 ($index) must be of type int, array given
stdClass TypeError/SplDoublyLinkedList::offsetGet(): Argument #1 ($index) must be of type int, stdClass given TypeError/SplDoublyLinkedList::offsetExists(): Argument #1 ($index) must be of type int, stdClass given TypeError/SplDoublyLinkedList::offsetSet(): Argument #1 ($index) must be of type ?int, stdClass given TypeError/SplDoublyLinkedList::offsetUnset(): Argument #1 ($index) must be of type int, stdClass given
SplQueue
bool "a" true ["c","b"] ["b"]
bool "b" true ["a","c"] ["a"]
null
Deprecated: SplDoublyLinkedList::offsetGet(): Passing null to parameter #1 ($index) of type int is deprecated in %s on line 13
 "a"
Deprecated: SplDoublyLinkedList::offsetExists(): Passing null to parameter #1 ($index) of type int is deprecated in %s on line 15
 true ["a","b","c"]
Deprecated: SplDoublyLinkedList::offsetUnset(): Passing null to parameter #1 ($index) of type int is deprecated in %s on line 20
 ["b"]
string "b" true ["a","c"] ["a"]
string "b" true ["a","c"] ["a"]
string TypeError/SplDoublyLinkedList::offsetGet(): Argument #1 ($index) must be of type int, string given TypeError/SplDoublyLinkedList::offsetExists(): Argument #1 ($index) must be of type int, string given TypeError/SplDoublyLinkedList::offsetSet(): Argument #1 ($index) must be of type ?int, string given TypeError/SplDoublyLinkedList::offsetUnset(): Argument #1 ($index) must be of type int, string given
int OutOfRangeException/SplDoublyLinkedList::offsetGet(): Argument #1 ($index) is out of range false OutOfRangeException/SplDoublyLinkedList::offsetSet(): Argument #1 ($index) is out of range OutOfRangeException/SplDoublyLinkedList::offsetUnset(): Argument #1 ($index) is out of range
int OutOfRangeException/SplDoublyLinkedList::offsetGet(): Argument #1 ($index) is out of range false OutOfRangeException/SplDoublyLinkedList::offsetSet(): Argument #1 ($index) is out of range OutOfRangeException/SplDoublyLinkedList::offsetUnset(): Argument #1 ($index) is out of range
float
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 13
 "b"
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 15
 true
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 17
 ["a","c"]
Deprecated: Implicit conversion from float 1.5 to int loses precision in %s on line 20
 ["a"]
array TypeError/SplDoublyLinkedList::offsetGet(): Argument #1 ($index) must be of type int, array given TypeError/SplDoublyLinkedList::offsetExists(): Argument #1 ($index) must be of type int, array given TypeError/SplDoublyLinkedList::offsetSet(): Argument #1 ($index) must be of type ?int, array given TypeError/SplDoublyLinkedList::offsetUnset(): Argument #1 ($index) must be of type int, array given
stdClass TypeError/SplDoublyLinkedList::offsetGet(): Argument #1 ($index) must be of type int, stdClass given TypeError/SplDoublyLinkedList::offsetExists(): Argument #1 ($index) must be of type int, stdClass given TypeError/SplDoublyLinkedList::offsetSet(): Argument #1 ($index) must be of type ?int, stdClass given TypeError/SplDoublyLinkedList::offsetUnset(): Argument #1 ($index) must be of type int, stdClass given
