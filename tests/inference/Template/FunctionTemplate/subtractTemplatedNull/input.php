<?php
/**
 * @template T
 * @param T|null $var
 * @return T
 */
function notNull($var) {
    if ($var === null) {
        throw new \InvalidArgumentException("");
    }

    return $var;
}

function takesNullableString(?string $s) : string {
    return notNull($s);
}