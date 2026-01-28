<?php
function takesInt(int $int): void { echo $int; }

function getIntOrNull(): ?int {
    return rand(0,1) === 0 ? null : 1;
}

/** @param mixed $value */
function assertNotNull($value): void {
    if (null === $value) {
        throw new Exception();
    }
}

$value = getIntOrNull();
assertNotNull($value);
takesInt($value);
