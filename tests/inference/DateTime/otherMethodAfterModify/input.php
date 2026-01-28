<?php
$datetime = new DateTime();
$dateTimeImmutable = new DateTimeImmutable();
$a = $datetime->modify("+1 day")->setTime(0, 0);
$b = $dateTimeImmutable->modify("+1 day")->setTime(0, 0);
