<?php
$date = new DateTimeImmutable("2024-03-30 02:30:00 Europe/Brussels");
$interval = new DateInterval("P1D");
$added = $date->add($interval);
$subtracted = $date->sub($interval);
echo $added->format('Y-m-d\TH:i:s.uP e I'), "\n";
echo $subtracted->format('Y-m-d\TH:i:s.uP e I'), "\n";
echo $date->diff($added)->format('%R%Y-%M-%D %H:%I:%S.%F %a %r %y %m %d %h %i %s %f %% %q %'), "\n";
echo $added->diff($date)->format('%R%Y-%M-%D %H:%I:%S.%F %a %r %y %m %d %h %i %s %f %% %q %'), "\n";
