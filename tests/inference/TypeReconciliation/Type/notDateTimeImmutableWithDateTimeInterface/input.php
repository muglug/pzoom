<?php
function foo(DateTimeInterface $dateTime): DateTimeInterface {
    $dateInterval = new DateInterval("P1D");

    if ($dateTime instanceof DateTimeImmutable) {
        return $dateTime->add($dateInterval);
    } else {
        $dateTime->add($dateInterval);

        return $dateTime;
    }
}
                