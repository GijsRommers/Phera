<?php
$start = new DateTimeImmutable('2024-10-25 13:00:01.123456 Europe/Brussels');
$end = $start->add(new DateInterval('P4D'));
$period = new DatePeriod($start, new DateInterval('P1D'), $end, DatePeriod::INCLUDE_END_DATE);
echo json_encode([$period instanceof IteratorAggregate, $period instanceof Traversable]), "\n";
foreach ($period as $key => $date) {
    echo $key, ' ', $date->format('Y-m-d\TH:i:s.uP e I'), "\n";
}
echo $period->current->format('Y-m-d\TH:i:s.uP e I'), "\n";
$iterator = $period->getIterator();
$iterator->rewind();
echo json_encode([$iterator->valid(), $iterator->key()]), "\n";
