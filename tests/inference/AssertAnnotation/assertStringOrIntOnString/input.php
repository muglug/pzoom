<?php
/**
 * @param mixed $v
 * @psalm-assert string|int $v
 */
function assertStringOrInt($v) : void {}

function gimmeAString(?string $v): string {
    /** @psalm-suppress TypeDoesNotContainType */
    assertStringOrInt($v);

    return $v;
}
