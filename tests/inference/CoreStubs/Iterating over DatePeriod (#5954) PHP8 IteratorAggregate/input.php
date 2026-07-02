<?php

$period = new DatePeriod(
    new DateTimeImmutable("now"),
    DateInterval::createFromDateString("1 day"),
    new DateTime("+1 week")
);
$dt = null;
foreach ($period as $dt) {
    echo $dt->format("Y-m-d");
}
