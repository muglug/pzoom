<?php
/**
 * @param interface-string<DateTimeInterface>|DateTimeInterface $maybe
 *
 * @return interface-string<DateTimeInterface>
 */
function Bar($maybe) : string {
    if (is_string($maybe)) {
        return $maybe;
    }

    return get_class($maybe);
}
