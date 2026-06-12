<?php
function &foo(): array {
    /** @var array $x */
    static $x = [1, 2, 3];
    return $x;
}
