<?php
/**
 * @param positive-int $i
 * @return positive-int
 */
function takesAnInt(int $i) {
    /** @psalm-suppress RedundantCast */
    return (int)$i;
}
