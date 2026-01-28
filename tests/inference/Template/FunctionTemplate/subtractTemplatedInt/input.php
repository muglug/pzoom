<?php
/**
 * @template T
 * @param T|int $var
 * @return T
 */
function notNull($var) {
    if (\is_int($var)) {
        throw new \InvalidArgumentException("");
    }

    return $var;
}

function takesNullableString(string|int $s) : string {
    return notNull($s);
}