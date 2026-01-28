<?php
function getString(): string
{
    return "";
}

$datetime = new DateTime();
$dateTimeImmutable = new DateTimeImmutable();
$a = $datetime->modify(getString());
$b = $dateTimeImmutable->modify(getString());
