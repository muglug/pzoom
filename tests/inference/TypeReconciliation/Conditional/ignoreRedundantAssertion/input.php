<?php
function gimmeAString(?string $v): string {
    /** @psalm-suppress TypeDoesNotContainType */
    assert(is_string($v) || is_object($v));

    return $v;
}