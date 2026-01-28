<?php
function foo(DateTimeInterface $dateTime): DateTimeInterface {
    $dateInterval = new DateInterval("P1D");

    if ($dateTime instanceof DateTime) {
        $dateTime->add($dateInterval);

        return $dateTime;
    } else {
        return $dateTime->add($dateInterval);
    }
}
                