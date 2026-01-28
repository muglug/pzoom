<?php
namespace Bar;

/** @psalm-pure */
function filterOdd(int $i) : ?int {
    if ($i % 2 === 0) {
        return $i;
    }

    return null;
}
