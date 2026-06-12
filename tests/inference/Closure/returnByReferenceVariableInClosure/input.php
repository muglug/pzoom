<?php
function &(): int {
    /** @var int $x */
    static $x = 1;
    return $x;
};
