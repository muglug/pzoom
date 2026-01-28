<?php
/**
 * @param string|object $maybe
 *
 * @throws InvalidArgumentException but it should not
 */
function foo($maybe) : string {
    /** @psalm-suppress DocblockTypeContradiction */
    if ( ! is_string($maybe) && ! is_object($maybe)) {
        throw new InvalidArgumentException("bad");
    }

    return is_string($maybe) ? $maybe : get_class($maybe);
}