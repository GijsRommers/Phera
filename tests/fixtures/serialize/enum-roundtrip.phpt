--TEST--
Enum records preserve their E wire bytes and singleton identity
--FILE--
<?php
enum QueueStatus { case Pending; case Complete; }
$record = ['current' => QueueStatus::Pending, 'history' => [QueueStatus::Complete, QueueStatus::Pending]];
$bytes = serialize($record);
echo bin2hex($bytes), "\n";
$copy = unserialize($bytes, ['allowed_classes' => false]);
var_dump($copy);
var_dump($copy['current'] === QueueStatus::Pending, $copy['current'] === $copy['history'][1]);
var_dump(unserialize('E:19:"QueueStatus:Pending";') === QueueStatus::Pending);
--EXPECT--
613a323a7b733a373a2263757272656e74223b453a31393a2251756575655374617475733a50656e64696e67223b733a373a22686973746f7279223b613a323a7b693a303b453a32303a2251756575655374617475733a436f6d706c657465223b693a313b723a323b7d7d
array(2) {
  ["current"]=>
  enum(QueueStatus::Pending)
  ["history"]=>
  array(2) {
    [0]=>
    enum(QueueStatus::Complete)
    [1]=>
    enum(QueueStatus::Pending)
  }
}
bool(true)
bool(true)
bool(true)
