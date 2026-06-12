<?php
/**
 * @psalm-assert iterable<string> $value
 * @param mixed $value
 *
 * @return void
 */
function assertAllString($value) : void {
    throw new \Exception(\var_export($value, true));
}

/**
 * @param ArrayIterator<string, mixed> $value
 *
 * @return ArrayIterator<string, string>
 */
function preserveContainerAllArrayIterator($value) {
    assertAllString($value);
    return $value;
}
