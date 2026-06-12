<?php
/** @var string $str */;
/** @var string|int $stringOrInt */;

if (isNonEmptyString($str)) {
    /** @psalm-check-type-exact $str = non-empty-string */;
} else {
    /** @psalm-check-type-exact $str = string */;
}

if (isNonEmptyString($stringOrInt)) {
    /** @psalm-check-type-exact $stringOrInt = non-empty-string */;
} else {
    /** @psalm-check-type-exact $stringOrInt = string|int */;
}

/**
 * @param mixed $_str
 * @psalm-assert-if-true non-empty-string $_str
 */
function isNonEmptyString($_str): bool
{
    return true;
}
