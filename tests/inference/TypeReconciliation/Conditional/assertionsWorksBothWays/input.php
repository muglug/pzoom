<?php
$a = 2;
$b = getPositiveInt();

assert($a === $b);

/** @return positive-int */
function getPositiveInt(): int{
    return 2;
}