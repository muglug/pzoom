<?php

/** @psalm-assert-if-true array<array-key, non-empty-string> $arr */
function assertArrayOfNonEmptyString(array $arr): bool
{
    foreach ($arr as $val) {
        if (!is_string($val) || $val === "") {
            return false;
        }
    }

    return true;
}

function guardedByContinue(array $arr): void
{
    foreach ($arr as $val) {
        if (!is_string($val)) {
            continue;
        }
        echo $val;
    }
}

function stillMixedWithoutGuard(array $arr): void
{
    foreach ($arr as $val) {
        if (is_string($val)) {
            echo $val;
        }
    }
}
