<?php
/**
 * @return "+1 day"|"+2 day"
 */
function getString(): string
{
    return "+1 day";
}

$datetime = new DateTime();
$dateTimeImmutable = new DateTimeImmutable();
$a = $datetime->modify(getString());
$b = $dateTimeImmutable->modify(getString());
