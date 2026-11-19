--TEST--
PHP classes can implement SplObserver and SplSubject
--FILE--
<?php
class Subject implements SplSubject {
    private SplObserver $observer;
    public function attach(SplObserver $observer): void { $this->observer = $observer; }
    public function detach(SplObserver $observer): void { unset($this->observer); }
    public function notify(): void { $this->observer->update($this); }
}
class Observer implements SplObserver {
    public function update(SplSubject $subject): void { echo get_class($subject), "\n"; }
}
$subject = new Subject;
$observer = new Observer;
$subject->attach($observer);
$subject->notify();
$subject->detach($observer);
--EXPECT--
Subject
