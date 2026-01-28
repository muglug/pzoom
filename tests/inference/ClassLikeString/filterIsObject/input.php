<?php
/**
 * @param interface-string<DateTimeInterface>|DateTimeInterface $maybe
 *
 * @return interface-string<DateTimeInterface>
 */
function Foo($maybe) : string {
    if (is_object($maybe)) {
        return get_class($maybe);
    }

    return $maybe;
}
