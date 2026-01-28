<?php
/**
 * @template T of scalar
 * @param T $value
 */
function normalizeValue(bool|int|float|string $value): void
{
    assert(is_string($value));
}