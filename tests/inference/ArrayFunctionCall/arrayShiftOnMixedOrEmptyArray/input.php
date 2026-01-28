<?php
/**
 * @param mixed|array<never, never> $lengths
 */
function doStuff($lengths): void {
    /** @psalm-suppress MixedArgument, MixedAssignment */
    $length = array_shift($lengths);
    if ($length !== null) {}
}
