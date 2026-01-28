<?php
/**
 * @psalm-pure
 */
function toDateTime(?DateTime $dateTime) : DateTime {
    assert($dateTime instanceof DateTime);
    return $dateTime;
}
