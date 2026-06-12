<?php

function tryModify(string $s): string
{
    if (@(new DateTime())->modify($s) === false) {
        return 'bad';
    }
    return 'good';
}

function modified(DateTime $dt, string $s): DateTime
{
    $result = $dt->modify($s);
    if ($result === false) {
        throw new UnexpectedValueException('bad modifier');
    }
    return $result;
}
