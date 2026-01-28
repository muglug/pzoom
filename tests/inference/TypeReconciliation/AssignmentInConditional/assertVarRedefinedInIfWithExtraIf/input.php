<?php
class O {}

/**
 * @param mixed $value
 */
function exampleWithOr($value): O {
    if (!is_string($value)) {
        return new O();
    }

    if (($value = rand(0, 1) ? new O : null) === null) {
        return new O();
    }

    return $value;
}