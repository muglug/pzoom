<?php

/** @psalm-assert non-empty-string $input */
function assertLowerCase(string $input): void { throw new \Exception($input . " irrelevant"); }

/**
 * @param lowercase-string $input
 * @return non-empty-lowercase-string
 */
function makeLowerNonEmpty(string $input): string
{
    assertLowerCase($input);

    return $input;
}
