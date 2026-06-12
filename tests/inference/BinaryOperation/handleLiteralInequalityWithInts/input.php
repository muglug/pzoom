<?php

/**
 * @param int<0, max> $i
 * @return int<1, max>
 */
function toPositiveInt(int $i): int
{
    if ($i !== 0) {
        return $i;
    }
    return 1;
}
