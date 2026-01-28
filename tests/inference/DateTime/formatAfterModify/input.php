<?php
$datetime = new DateTime();
$dateTimeImmutable = new DateTimeImmutable();
$a = $datetime->modify("+1 day")->format("Y-m-d");
$b = $dateTimeImmutable->modify("+1 day")->format("Y-m-d");
