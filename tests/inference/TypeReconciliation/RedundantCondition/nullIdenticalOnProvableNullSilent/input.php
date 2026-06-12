<?php

/**
 * @param array<array-key, mixed>|scalar|null $value
 */
function eliminateScalars($value): string
{
    if (is_array($value)) {
        return 'array';
    }

    if (is_string($value)) {
        return 'string';
    }

    if (is_int($value)) {
        return 'int';
    }

    if (is_float($value)) {
        return 'float';
    }

    if ($value === false) {
        return 'false';
    }

    if ($value === true) {
        return 'true';
    }

    if ($value === null) {
        return 'null';
    }

    return 'other';
}

function literalNullCheck(): string
{
    $x = null;
    if ($x === null) {
        return 'y';
    }
    return 'n';
}
