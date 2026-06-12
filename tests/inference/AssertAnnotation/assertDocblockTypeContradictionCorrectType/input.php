<?php
function takesAnInt(int $i): void {}

function takesAFloat(float $i): void {}

$foo = rand() / 2;

/** @psalm-suppress TypeDoesNotContainType */
if (is_int($foo) || !is_float($foo)) {
    takesAnInt($foo);
    exit;
}

takesAFloat($foo);
