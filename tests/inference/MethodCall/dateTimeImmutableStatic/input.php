<?php
final class MyDate extends DateTimeImmutable {}

$today = new MyDate();
$yesterday = $today->sub(new DateInterval("P1D"));

$b = (new DateTimeImmutable())->modify("+3 hours");
