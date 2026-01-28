<?php
/**
 * @param string|object $maybe
 *
 * @throws InvalidArgumentException but it should not
 */
function bar($maybe) : string {
    /** @psalm-suppress DocblockTypeContradiction */
    if ( ! is_string($maybe) && ! is_object($maybe)) {
        throw new InvalidArgumentException("bad");
    }

    return is_object($maybe) ? get_class($maybe) : $maybe;
}